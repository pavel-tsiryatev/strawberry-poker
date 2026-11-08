//! Игровой сервер. Запускается у хоста в отдельном потоке (или отдельной
//! программой) и ведёт игру авторитарно: клиенты только присылают действия.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use nanoserde::{DeJson, SerJson};

use crate::ai;
use crate::cards::Rng;
use crate::game::{Action, BotStyle, Game, Player};
use crate::proto::*;

const TURN_SECS: u64 = 30;
const RESULT_SECS: f32 = 4.5;
const RUNOUT_SECS: f32 = 1.3;

pub const BOT_NAMES: [&str; 16] = [
    "Марина", "Viktor", "Луна", "Shark", "Ольга", "Бублик", "Max", "Ника", "Тимур", "Lady J", "Гоша",
    "Ace", "Соня", "Фокусник", "Dasha", "Борис",
];

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub port: u16,
    pub sb: u64,
    pub bb: u64,
    pub stack: u64,
    /// Сколько ботов посадить сразу
    pub bots: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { port: DEFAULT_PORT, sb: 50, bb: 100, stack: 10_000, bots: 0 }
    }
}

pub struct ServerHandle {
    pub port: u16,
    /// Ключ хоста: клиент с этим ключом получает права управления столом
    pub key: String,
    stop: Arc<AtomicBool>,
}

impl ServerHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

enum Ev {
    Join(u32, TcpStream),
    Msg(u32, ClientMsg),
    Leave(u32),
}

struct Conn {
    id: u32,
    stream: TcpStream,
    seat: Option<usize>,
    host: bool,
    hello: bool,
    name: String,
    avatar: u32,
}

struct Server {
    cfg: ServerConfig,
    key: String,
    game: Game,
    conns: Vec<Conn>,
    /// Какое соединение сидит на месте
    seat_conn: [Option<u32>; MAX_SEATS],
    /// Место освободится после раздачи
    leaving: [bool; MAX_SEATS],
    started: bool,
    rng: Rng,
    seen: (u64, u64),
    turn_started: Instant,
    think: Duration,
    result_at: Option<Instant>,
    dirty: bool,
    status: String,
}

/// Запускает сервер в фоне.
pub fn start(cfg: ServerConfig) -> std::io::Result<ServerHandle> {
    let listener = TcpListener::bind(("0.0.0.0", cfg.port))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let mut rng = Rng::from_time();
    let key = format!("{:016x}", rng.next_u64());
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();

    // Поток приёма соединений
    {
        let stop = stop.clone();
        let tx = tx.clone();
        thread::spawn(move || {
            let mut next_id = 1u32;
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_nodelay(true);
                        let id = next_id;
                        next_id += 1;
                        if let Ok(read) = stream.try_clone() {
                            spawn_reader(id, read, tx.clone());
                            if tx.send(Ev::Join(id, stream)).is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => thread::sleep(Duration::from_millis(30)),
                }
            }
        });
    }

    let mut server = Server::new(cfg, key.clone(), rng);
    let stop2 = stop.clone();
    thread::spawn(move || server.run(rx, stop2));
    Ok(ServerHandle { port, key, stop })
}

fn spawn_reader(id: u32, stream: TcpStream, tx: Sender<Ev>) {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if line.len() > 4096 {
                break;
            }
            if let Ok(msg) = ClientMsg::deserialize_json(&line) {
                if tx.send(Ev::Msg(id, msg)).is_err() {
                    return;
                }
            }
        }
        let _ = tx.send(Ev::Leave(id));
    });
}

fn empty_seat() -> Player {
    let mut p = Player::new("", 0, false, BotStyle { tight: 0.5, aggr: 0.5, bluff: 0.0 }, 0);
    p.seated = false;
    p
}

impl Server {
    fn new(cfg: ServerConfig, key: String, mut rng: Rng) -> Self {
        let players = (0..MAX_SEATS).map(|_| empty_seat()).collect();
        let game = Game::new(players, cfg.sb, cfg.bb, Rng::new(rng.next_u64()));
        let mut s = Server {
            cfg,
            key,
            game,
            conns: Vec::new(),
            seat_conn: [None; MAX_SEATS],
            leaving: [false; MAX_SEATS],
            started: false,
            rng,
            seen: (0, 0),
            turn_started: Instant::now(),
            think: Duration::from_millis(900),
            result_at: None,
            dirty: true,
            status: String::new(),
        };
        for _ in 0..s.cfg.bots.min(MAX_SEATS - 1) {
            s.add_bot();
        }
        s
    }

    fn run(&mut self, rx: Receiver<Ev>, stop: Arc<AtomicBool>) {
        while !stop.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(40)) {
                Ok(ev) => self.handle(ev),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
            while let Ok(ev) = rx.try_recv() {
                self.handle(ev);
            }
            self.tick();
            if self.dirty {
                self.dirty = false;
                self.broadcast();
            }
        }
        for c in &self.conns {
            let _ = c.stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn hand_running(&self) -> bool {
        self.game.result.is_none() && self.game.players.iter().any(|p| p.in_hand)
    }

    fn free_seat(&self) -> Option<usize> {
        // Человек садится как можно «напротив» — просто первое свободное место
        (0..MAX_SEATS).find(|&i| !self.game.players[i].seated)
    }

    fn add_bot(&mut self) -> bool {
        let Some(seat) = self.free_seat() else { return false };
        let taken: Vec<&str> = self.game.players.iter().filter(|p| p.seated).map(|p| p.name.as_str()).collect();
        let free: Vec<&str> = BOT_NAMES.iter().copied().filter(|n| !taken.contains(n)).collect();
        let name = if free.is_empty() { "Бот" } else { free[self.rng.below(free.len())] };
        let style = BotStyle { tight: self.rng.unit(), aggr: self.rng.unit(), bluff: self.rng.unit() * 0.18 };
        let avatar = self.rng.below(10);
        let mut p = Player::new(name, self.cfg.stack, false, style, avatar);
        p.in_hand = false;
        self.game.players[seat] = p;
        self.leaving[seat] = false;
        self.seat_conn[seat] = None;
        self.game_log(format!("{} (бот) садится за стол", name));
        self.dirty = true;
        true
    }

    fn game_log(&mut self, s: String) {
        self.game.log.push(s);
        if self.game.log.len() > 50 {
            self.game.log.remove(0);
        }
    }

    /// Убрать игрока с места (или отложить до конца раздачи).
    fn vacate(&mut self, seat: usize) {
        let p = &self.game.players[seat];
        if self.hand_running() && p.in_hand {
            self.leaving[seat] = true;
        } else {
            self.game.players[seat] = empty_seat();
            self.leaving[seat] = false;
        }
        self.seat_conn[seat] = None;
        self.dirty = true;
    }

    /// Посадить зрителя на место (освободив бота, если мест нет).
    fn try_seat(&mut self, ci: usize) {
        if self.conns[ci].seat.is_some() {
            return;
        }
        let mut seat = self.free_seat();
        if seat.is_none() {
            // заменяем бота, который не в раздаче
            seat = (0..MAX_SEATS).find(|&i| {
                let p = &self.game.players[i];
                p.seated && !p.is_human && !(self.hand_running() && p.in_hand) && !self.leaving[i]
            });
        }
        let Some(seat) = seat else { return };
        let c = &self.conns[ci];
        let mut p = Player::new(
            &c.name,
            self.cfg.stack,
            true,
            BotStyle { tight: 0.5, aggr: 0.5, bluff: 0.0 },
            c.avatar as usize,
        );
        p.in_hand = false;
        let name = c.name.clone();
        let id = c.id;
        self.game.players[seat] = p;
        self.seat_conn[seat] = Some(id);
        self.leaving[seat] = false;
        self.conns[ci].seat = Some(seat);
        self.game_log(format!("{} садится за стол", name));
        self.dirty = true;
    }

    fn conn_index(&self, id: u32) -> Option<usize> {
        self.conns.iter().position(|c| c.id == id)
    }

    fn send(&self, ci: usize, msg: &ServerMsg) -> bool {
        let mut line = msg.serialize_json();
        line.push('\n');
        (&self.conns[ci].stream).write_all(line.as_bytes()).is_ok()
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Join(id, stream) => {
                let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                self.conns.push(Conn {
                    id,
                    stream,
                    seat: None,
                    host: false,
                    hello: false,
                    name: String::new(),
                    avatar: 0,
                });
            }
            Ev::Leave(id) => {
                if let Some(ci) = self.conn_index(id) {
                    let c = self.conns.remove(ci);
                    if let Some(seat) = c.seat {
                        let name = c.name.clone();
                        self.vacate(seat);
                        self.game_log(format!("{} покидает стол", name));
                    }
                    self.dirty = true;
                }
            }
            Ev::Msg(id, msg) => {
                if let Some(ci) = self.conn_index(id) {
                    self.on_msg(ci, msg);
                }
            }
        }
    }

    fn on_msg(&mut self, ci: usize, m: ClientMsg) {
        let is_host = self.conns[ci].host;
        match m.t.as_str() {
            "hello" => {
                if self.conns[ci].hello {
                    return;
                }
                if m.version != PROTO_VERSION {
                    let msg = ServerMsg {
                        t: "error".into(),
                        view: None,
                        text: "Разные версии игры у хоста и у вас".into(),
                    };
                    self.send(ci, &msg);
                    let _ = self.conns[ci].stream.shutdown(std::net::Shutdown::Both);
                    return;
                }
                let mut name: String = m.name.trim().chars().filter(|c| !c.is_control()).take(14).collect();
                if name.is_empty() {
                    name = format!("Игрок {}", self.conns[ci].id);
                }
                // одинаковые имена различаем
                while self.game.players.iter().any(|p| p.seated && p.name == name) {
                    name.push('\'');
                }
                let c = &mut self.conns[ci];
                c.hello = true;
                c.name = name;
                c.avatar = m.avatar % 10;
                c.host = !self.key.is_empty() && m.key == self.key;
                self.try_seat(ci);
                self.dirty = true;
            }
            "act" => {
                let Some(seat) = self.conns[ci].seat else { return };
                if self.game.to_act == Some(seat) && self.game.result.is_none() {
                    let a = match m.kind {
                        0 => Action::Fold,
                        1 => Action::Check,
                        2 => Action::Call,
                        _ => Action::RaiseTo(m.amount),
                    };
                    self.game.act(a);
                }
            }
            "add_bot" if is_host => {
                self.add_bot();
            }
            "kick" if is_host => {
                let seat = m.seat as usize;
                if seat < MAX_SEATS && self.game.players[seat].seated && !self.game.players[seat].is_human {
                    let name = self.game.players[seat].name.clone();
                    self.vacate(seat);
                    self.game_log(format!("{} уходит", name));
                }
            }
            "start" if is_host => {
                self.started = true;
                self.dirty = true;
            }
            "rebuy" => {
                let Some(seat) = self.conns[ci].seat else { return };
                let p = &mut self.game.players[seat];
                if p.chips == 0 && !(p.in_hand && self.game.result.is_none()) {
                    p.chips = self.cfg.stack;
                    let name = p.name.clone();
                    self.game_log(format!("{} докупается на {}", name, crate::fmt_chips(self.cfg.stack)));
                    self.dirty = true;
                }
            }
            "sit" => self.try_seat(ci),
            _ => {}
        }
    }

    fn between_hands(&mut self) {
        for i in 0..MAX_SEATS {
            if self.leaving[i] {
                self.game.players[i] = empty_seat();
                self.leaving[i] = false;
            }
            let p = &mut self.game.players[i];
            if p.seated && !p.is_human && p.chips == 0 {
                // проигравший бот уходит, вместо него садится новый
                let name = p.name.clone();
                self.game.players[i] = empty_seat();
                self.game_log(format!("{} вылетает", name));
                self.add_bot();
            }
        }
        // зрители садятся на освободившиеся места
        for ci in 0..self.conns.len() {
            if self.conns[ci].hello && self.conns[ci].seat.is_none() {
                self.try_seat(ci);
            }
        }
        self.result_at = None;
        self.game.result = None;
        for p in &mut self.game.players {
            p.in_hand = false;
            p.hole.clear();
            p.bet = 0;
            p.last_action = None;
        }
        self.game.board.clear();
        self.dirty = true;
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.started {
            if let Some(t) = self.result_at {
                if now.duration_since(t).as_secs_f32() > RESULT_SECS {
                    self.between_hands();
                }
            } else if !self.hand_running() {
                if self.game.active_count() >= 2 {
                    self.game.start_hand();
                    self.status.clear();
                } else {
                    let st = "Ждём игроков…".to_string();
                    if self.status != st {
                        self.status = st;
                        self.dirty = true;
                    }
                }
            } else if self.game.needs_runout() {
                if now.duration_since(self.turn_started).as_secs_f32() > RUNOUT_SECS {
                    self.game.runout_step();
                }
            } else if let Some(seat) = self.game.to_act {
                let p = &self.game.players[seat];
                let elapsed = now.duration_since(self.turn_started);
                let auto = |g: &Game| if g.to_call(seat) == 0 { Action::Check } else { Action::Fold };
                if p.is_human {
                    let gone = self.seat_conn[seat].is_none();
                    if gone || elapsed > Duration::from_secs(TURN_SECS) {
                        let a = auto(&self.game);
                        self.game.act(a);
                    }
                } else if elapsed > self.think {
                    let a = ai::decide(&mut self.game, seat);
                    self.game.act(a);
                }
            }
        }

        let key = (self.game.hand_no, self.game.seq);
        if key != self.seen {
            self.seen = key;
            self.turn_started = now;
            self.think = Duration::from_millis(700 + self.rng.below(1100) as u64);
            if self.game.result.is_some() && self.result_at.is_none() {
                self.result_at = Some(now);
            }
            self.dirty = true;
        }
    }

    fn view_for(&self, ci: usize) -> View {
        let c = &self.conns[ci];
        let g = &self.game;
        let you = c.seat.map(|s| s as i32).unwrap_or(-1);
        let showdown = g.result.as_ref().map(|r| r.showdown).unwrap_or(false);
        let runout = g.needs_runout();
        let seats = g
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let visible = (Some(i) == c.seat)
                    || (showdown && p.in_hand && !p.folded)
                    || (runout && p.contesting());
                SeatView {
                    occupied: p.seated,
                    name: p.name.clone(),
                    avatar: p.avatar as u32,
                    chips: p.chips,
                    bet: p.bet,
                    folded: p.folded,
                    all_in: p.all_in,
                    in_hand: p.in_hand,
                    is_bot: p.seated && !p.is_human,
                    connected: !p.is_human || self.seat_conn[i].is_some(),
                    has_cards: !p.hole.is_empty() && !p.folded,
                    cards: if visible { p.hole.iter().map(|&c| card_code(c)).collect() } else { vec![] },
                    last_action: p.last_action.clone().unwrap_or_default(),
                }
            })
            .collect();
        let mine = c.seat.filter(|&s| g.to_act == Some(s));
        let turn_total = if g.to_act.map(|s| g.players[s].is_human).unwrap_or(false) {
            Duration::from_secs(TURN_SECS)
        } else {
            self.think
        };
        let left = turn_total.saturating_sub(self.turn_started.elapsed());
        View {
            you,
            host: c.host,
            started: self.started,
            seats,
            board: g.board.iter().map(|&c| card_code(c)).collect(),
            pot: g.total_pot(),
            dealer: g.dealer as i32,
            to_act: g.to_act.map(|s| s as i32).unwrap_or(-1),
            turn_left_ms: left.as_millis() as u32,
            turn_total_ms: turn_total.as_millis() as u32,
            current_bet: g.current_bet,
            to_call: mine.map(|s| g.to_call(s)).unwrap_or(0),
            min_raise_to: mine.map(|s| g.min_raise_to(s)).unwrap_or(0),
            max_raise_to: mine.map(|s| g.max_raise_to(s)).unwrap_or(0),
            can_raise: mine.map(|s| g.can_raise(s)).unwrap_or(false),
            hand_no: g.hand_no,
            seq: g.seq,
            runout,
            result: g.result.as_ref().map(|r| ResultView {
                wins: r
                    .wins
                    .iter()
                    .map(|w| WinView {
                        seat: w.seat as u32,
                        amount: w.amount,
                        hand: w.hand.map(|h| h.describe()).unwrap_or_default(),
                    })
                    .collect(),
                showdown: r.showdown,
                best_cards: r.best_cards.iter().map(|&c| card_code(c)).collect(),
                text: r.text.clone(),
            }),
            log: g.log.iter().rev().take(8).rev().cloned().collect(),
            sb: g.sb,
            bb: g.bb,
            stack: self.cfg.stack,
            status: self.status.clone(),
        }
    }

    fn broadcast(&mut self) {
        let mut dead = Vec::new();
        for ci in 0..self.conns.len() {
            if !self.conns[ci].hello {
                continue;
            }
            let msg = ServerMsg { t: "view".into(), view: Some(self.view_for(ci)), text: String::new() };
            if !self.send(ci, &msg) {
                dead.push(self.conns[ci].id);
            }
        }
        for id in dead {
            if let Some(ci) = self.conn_index(id) {
                let _ = self.conns[ci].stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}
