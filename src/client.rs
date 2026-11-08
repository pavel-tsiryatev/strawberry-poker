//! Сетевой клиент: подключение в фоне, чтение сообщений в отдельном потоке.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use nanoserde::{DeJson, SerJson};

use crate::proto::*;

pub enum ClientEvent {
    Connected,
    View(Box<View>),
    Error(String),
    Disconnected(String),
}

pub struct Client {
    stream: Arc<Mutex<Option<TcpStream>>>,
    pub rx: Receiver<ClientEvent>,
}

impl Client {
    /// Подключается к `addr` (хост или хост:порт) в фоне и представляется.
    pub fn connect(addr: &str, hello: ClientMsg) -> Client {
        let (tx, rx) = channel();
        let stream: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        let slot = stream.clone();
        let addr = if addr.contains(':') { addr.to_string() } else { format!("{}:{}", addr, DEFAULT_PORT) };
        thread::spawn(move || {
            let target = match addr.to_socket_addrs().ok().and_then(|mut a| a.next()) {
                Some(a) => a,
                None => {
                    let _ = tx.send(ClientEvent::Error(format!("Неверный адрес: {}", addr)));
                    return;
                }
            };
            let s = match TcpStream::connect_timeout(&target, Duration::from_secs(5)) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(ClientEvent::Error(format!("Не удалось подключиться: {}", e)));
                    return;
                }
            };
            let _ = s.set_nodelay(true);
            let mut line = hello.serialize_json();
            line.push('\n');
            if (&s).write_all(line.as_bytes()).is_err() {
                let _ = tx.send(ClientEvent::Error("Соединение оборвалось".into()));
                return;
            }
            let read = match s.try_clone() {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(ClientEvent::Error(e.to_string()));
                    return;
                }
            };
            *slot.lock().unwrap() = Some(s);
            let _ = tx.send(ClientEvent::Connected);
            let reader = BufReader::new(read);
            let mut reason = "Соединение с хостом потеряно".to_string();
            for line in reader.lines() {
                let Ok(line) = line else { break };
                match ServerMsg::deserialize_json(&line) {
                    Ok(m) if m.t == "view" => {
                        if let Some(v) = m.view {
                            if tx.send(ClientEvent::View(Box::new(v))).is_err() {
                                return;
                            }
                        }
                    }
                    Ok(m) if m.t == "error" => {
                        reason = m.text;
                    }
                    _ => {}
                }
            }
            let _ = tx.send(ClientEvent::Disconnected(reason));
        });
        Client { stream, rx }
    }

    pub fn send(&self, msg: &ClientMsg) {
        let mut line = msg.serialize_json();
        line.push('\n');
        if let Some(s) = self.stream.lock().unwrap().as_ref() {
            let _ = (&*s).write_all(line.as_bytes());
        }
    }

    pub fn close(&self) {
        if let Some(s) = self.stream.lock().unwrap().as_ref() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.close();
    }
}

/// Локальные IPv4-адреса машины (чтобы хост мог сообщить их друзьям).
pub fn local_ips() -> Vec<String> {
    let mut out = Vec::new();
    // Трюк: «подключаем» UDP-сокет к внешнему адресу — пакеты не отправляются,
    // но ОС выбирает интерфейс и мы узнаём свой адрес в локальной сети.
    for probe in ["8.8.8.8:80", "10.255.255.255:1", "192.168.255.255:1"] {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(probe).is_ok() {
                if let Ok(a) = sock.local_addr() {
                    let ip = a.ip().to_string();
                    if ip != "0.0.0.0" && !out.contains(&ip) {
                        out.push(ip);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{self, ServerConfig};
    use std::time::Instant;

    fn wait_view(c: &Client, pred: impl Fn(&View) -> bool) -> View {
        let t = Instant::now();
        loop {
            assert!(t.elapsed() < Duration::from_secs(20), "таймаут ожидания");
            match c.rx.recv_timeout(Duration::from_millis(100)) {
                Ok(ClientEvent::View(v)) if pred(&v) => return *v,
                Ok(ClientEvent::Error(e)) | Ok(ClientEvent::Disconnected(e)) => panic!("{}", e),
                _ => {}
            }
        }
    }

    #[test]
    fn two_players_and_bot_play_over_tcp() {
        let h = server::start(ServerConfig { port: 0, bots: 1, ..Default::default() }).unwrap();
        let addr = format!("127.0.0.1:{}", h.port);
        let mut hello = ClientMsg::new("hello");
        hello.version = PROTO_VERSION;
        hello.name = "Хост".into();
        hello.key = h.key.clone();
        let host = Client::connect(&addr, hello.clone());
        let v = wait_view(&host, |v| v.you >= 0);
        assert!(v.host);

        hello.name = "Друг".into();
        hello.key = String::new();
        let friend = Client::connect(&addr, hello);
        let fv = wait_view(&friend, |v| v.you >= 0);
        assert!(!fv.host);
        assert_eq!(fv.seats.iter().filter(|s| s.occupied).count(), 3);

        // друг не может начать игру, хост — может
        friend.send(&ClientMsg::new("start"));
        host.send(&ClientMsg::new("start"));
        let v = wait_view(&host, |v| v.hand_no >= 1 && !v.board.is_empty() || v.to_act == v.you);
        // свои карты видны, чужие — нет
        let me = v.you as usize;
        assert_eq!(v.seats[me].cards.len(), 2);
        for (i, s) in v.seats.iter().enumerate() {
            if i != me && v.result.is_none() && !v.runout {
                assert!(s.cards.is_empty());
            }
        }

        // играем: оба человека всегда коллируют/чекают, пока не сыграют 2 раздачи
        let t = Instant::now();
        let mut hands_done = 0;
        let mut last_hand = 0;
        let mut first = Some(v.clone());
        while hands_done < 2 {
            assert!(t.elapsed() < Duration::from_secs(60));
            for (k, c) in [&host, &friend].into_iter().enumerate() {
                let mut evs: Vec<ClientEvent> = Vec::new();
                if k == 0 {
                    if let Some(v) = first.take() {
                        evs.push(ClientEvent::View(Box::new(v)));
                    }
                }
                while let Ok(ev) = c.rx.try_recv() {
                    evs.push(ev);
                }
                for ev in evs {
                    if let ClientEvent::View(v) = ev {
                        if v.to_act == v.you && v.you >= 0 && v.result.is_none() {
                            let mut m = ClientMsg::new("act");
                            m.kind = 2;
                            c.send(&m);
                        }
                        if v.result.is_some() && v.hand_no != last_hand {
                            last_hand = v.hand_no;
                            hands_done += 1;
                            let total: u64 = v.seats.iter().map(|s| s.chips).sum();
                            assert_eq!(total, 30_000, "фишки не сходятся");
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        h.stop();
    }
}
