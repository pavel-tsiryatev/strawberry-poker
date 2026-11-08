//! Strawberry Poker — 3D Texas Hold'em для игры с друзьями по сети.
#![allow(clippy::too_many_arguments)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gfx;
mod scene;

use gfx::*;
use macroquad::prelude::*;
use scene::{RX, RZ, Scene};
use strawberry_poker::client::{Client, ClientEvent, local_ips};
use strawberry_poker::eval::evaluate;
use strawberry_poker::proto::*;
use strawberry_poker::server::{self, ServerConfig, ServerHandle};

const SEAT_ANGLES: [f32; MAX_SEATS] = [90.0, 150.0, 210.0, 270.0, 330.0, 30.0];
const AVATAR_COLORS: [u32; 10] = [
    0xe8505b, 0xf9a03f, 0x4cc38a, 0x3d8bfd, 0xa66cff, 0xff6fb5, 0x2ec4d6, 0xc0ca33, 0xff8a65, 0x7e57c2,
];
const BLINDS: [(u64, u64); 6] = [(10, 20), (25, 50), (50, 100), (100, 200), (250, 500), (1000, 2000)];
const STACK_BB: [u64; 4] = [50, 100, 200, 500];

// ---------------------------------------------------------------- настройки

struct Settings {
    name: String,
    avatar: u32,
    last_ip: String,
    blinds: usize,
    stack: usize,
    bots: usize,
}

fn data_file() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "android")]
    {
        let dir = std::path::PathBuf::from("/data/data/rust.strawberry_poker/files");
        let _ = std::fs::create_dir_all(&dir);
        return Some(dir.join("settings.txt"));
    }
    #[allow(unreachable_code)]
    {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        Some(std::path::PathBuf::from(home).join(".strawberry_poker"))
    }
}

impl Settings {
    fn load() -> Settings {
        let seed = (miniquad::date::now() * 1000.0) as u64;
        let mut s = Settings {
            name: String::new(),
            avatar: (seed % 10) as u32,
            last_ip: String::new(),
            blinds: 2,
            stack: 1,
            bots: 2,
        };
        if let Some(text) = data_file().and_then(|p| std::fs::read_to_string(p).ok()) {
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else { continue };
                match k {
                    "name" => s.name = v.to_string(),
                    "avatar" => s.avatar = v.parse().unwrap_or(0) % 10,
                    "ip" => s.last_ip = v.to_string(),
                    "blinds" => s.blinds = v.parse().unwrap_or(2).min(BLINDS.len() - 1),
                    "stack" => s.stack = v.parse().unwrap_or(1).min(STACK_BB.len() - 1),
                    "bots" => s.bots = v.parse().unwrap_or(2).min(MAX_SEATS - 1),
                    _ => {}
                }
            }
        }
        if s.name.is_empty() {
            const NAMES: [&str; 8] = ["Клубничка", "Туз", "Блефер", "Везунчик", "Фишка", "Джокер", "Ривер", "Флоп"];
            s.name = NAMES[(seed / 10 % 8) as usize].to_string();
        }
        s
    }

    fn store(&self) {
        if let Some(p) = data_file() {
            let _ = std::fs::write(
                p,
                format!(
                    "name={}\navatar={}\nip={}\nblinds={}\nstack={}\nbots={}\n",
                    self.name, self.avatar, self.last_ip, self.blinds, self.stack, self.bots
                ),
            );
        }
    }
}

// ---------------------------------------------------------------- состояние

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Menu,
    Host,
    Join,
    Table,
}

#[derive(PartialEq, Clone, Copy)]
enum Field {
    None,
    Name,
    Ip,
}

struct Net {
    client: Client,
    server: Option<ServerHandle>,
    view: Option<View>,
    recv_time: f64,
    connected: bool,
    ips: Vec<String>,
    port: u16,
}

#[derive(Default)]
struct Anim {
    hand: u64,
    deal_time: f64,
    board_times: Vec<f64>,
    reveal_times: [f64; MAX_SEATS],
    result_time: f64,
    had_result: bool,
}

struct App {
    screen: Screen,
    settings: Settings,
    field: Field,
    net: Option<Net>,
    message: Option<String>,
    anim: Anim,
    raise_open: bool,
    raise_to: u64,
    dragging: bool,
    confirm_leave: bool,
    scene: Scene,
    last_turn_key: (u64, u64),
}

fn ellipse_point(d: usize, kx: f32, kz: f32, extra_deg: f32) -> Vec3 {
    let a = (SEAT_ANGLES[d] + extra_deg).to_radians();
    vec3(kx * a.cos(), 0.0, kz * a.sin())
}

fn seat_world(d: usize) -> Vec3 {
    ellipse_point(d, RX + 0.95, RZ + 1.0, 0.0) + Vec3::Y * 0.4
}
fn bet_world(d: usize) -> Vec3 {
    ellipse_point(d, RX * 0.6, RZ * 0.55, 0.0)
}
fn hole_world(d: usize) -> Vec3 {
    ellipse_point(d, RX * 0.8, RZ * 0.76, 0.0)
}
fn dealer_world(d: usize) -> Vec3 {
    ellipse_point(d, RX * 0.76, RZ * 0.70, if d == 0 { -16.0 } else { 15.0 })
}
const POT_WORLD: Vec3 = Vec3::new(0.0, 0.0, -1.2);
const DECK_WORLD: Vec3 = Vec3::new(0.0, 0.3, -1.0);

fn board_world(i: usize) -> Vec3 {
    vec3((i as f32 - 2.0) * 1.12, 0.0, 0.3)
}

fn seat_disp(you: i32, seat: usize) -> usize {
    if you >= 0 { (seat + MAX_SEATS - you as usize) % MAX_SEATS } else { seat }
}

impl App {
    fn new(scene: Scene) -> Self {
        App {
            screen: Screen::Menu,
            settings: Settings::load(),
            field: Field::None,
            net: None,
            message: None,
            anim: Anim::default(),
            raise_open: false,
            raise_to: 0,
            dragging: false,
            confirm_leave: false,
            scene,
            last_turn_key: (0, 0),
        }
    }

    fn hello(&self, key: &str) -> ClientMsg {
        let mut m = ClientMsg::new("hello");
        m.version = PROTO_VERSION;
        m.name = self.settings.name.clone();
        m.avatar = self.settings.avatar;
        m.key = key.to_string();
        m
    }

    fn host_game(&mut self) {
        let (sb, bb) = BLINDS[self.settings.blinds];
        let cfg = ServerConfig {
            port: DEFAULT_PORT,
            sb,
            bb,
            stack: bb * STACK_BB[self.settings.stack],
            bots: self.settings.bots,
        };
        self.settings.store();
        // если стандартный порт занят — берём любой свободный
        let handle = server::start(cfg.clone()).or_else(|_| server::start(ServerConfig { port: 0, ..cfg }));
        match handle {
            Ok(h) => {
                let client = Client::connect(&format!("127.0.0.1:{}", h.port), self.hello(&h.key));
                self.net = Some(Net {
                    client,
                    port: h.port,
                    server: Some(h),
                    view: None,
                    recv_time: 0.0,
                    connected: false,
                    ips: local_ips(),
                });
                self.enter_table();
            }
            Err(e) => self.message = Some(format!("Не удалось создать стол: {}", e)),
        }
    }

    fn join_game(&mut self) {
        let ip = self.settings.last_ip.trim().to_string();
        if ip.is_empty() {
            self.message = Some("Введите адрес хоста".into());
            return;
        }
        self.settings.store();
        let client = Client::connect(&ip, self.hello(""));
        self.net = Some(Net {
            client,
            server: None,
            view: None,
            recv_time: 0.0,
            connected: false,
            ips: Vec::new(),
            port: DEFAULT_PORT,
        });
        self.enter_table();
    }

    fn enter_table(&mut self) {
        self.screen = Screen::Table;
        self.anim = Anim::default();
        self.raise_open = false;
        self.confirm_leave = false;
        self.message = None;
        self.set_field(Field::None);
    }

    fn leave(&mut self, reason: Option<String>) {
        if let Some(net) = self.net.take() {
            net.client.close();
            if let Some(s) = &net.server {
                s.stop();
            }
        }
        self.message = reason;
        self.screen = Screen::Menu;
    }

    fn set_field(&mut self, f: Field) {
        if self.field != f {
            self.field = f;
            miniquad::window::show_keyboard(f != Field::None);
        }
    }

    fn send(&self, m: ClientMsg) {
        if let Some(n) = &self.net {
            n.client.send(&m);
        }
    }

    fn act(&mut self, kind: u32, amount: u64) {
        let mut m = ClientMsg::new("act");
        m.kind = kind;
        m.amount = amount;
        self.send(m);
        self.raise_open = false;
    }

    // ---------------------------------------------------------- сеть

    fn poll(&mut self) {
        let now = get_time();
        let mut events = Vec::new();
        if let Some(n) = &self.net {
            while let Ok(ev) = n.client.rx.try_recv() {
                events.push(ev);
            }
        }
        for ev in events {
            match ev {
                ClientEvent::Connected => {
                    if let Some(n) = &mut self.net {
                        n.connected = true;
                    }
                }
                ClientEvent::View(v) => self.on_view(*v, now),
                ClientEvent::Error(e) | ClientEvent::Disconnected(e) => {
                    self.leave(Some(e));
                    return;
                }
            }
        }
    }

    fn on_view(&mut self, v: View, now: f64) {
        let a = &mut self.anim;
        if v.hand_no != a.hand {
            a.hand = v.hand_no;
            a.deal_time = now;
            a.board_times.clear();
            a.reveal_times = [0.0; MAX_SEATS];
            a.had_result = false;
        }
        if v.board.len() < a.board_times.len() {
            a.board_times.clear();
        }
        while a.board_times.len() < v.board.len() {
            let i = a.board_times.len();
            let delay = if i < 3 { i as f64 * 0.15 } else { 0.0 };
            a.board_times.push(now + delay);
        }
        for (i, s) in v.seats.iter().enumerate() {
            if s.cards.is_empty() {
                a.reveal_times[i] = 0.0;
            } else if a.reveal_times[i] == 0.0 {
                a.reveal_times[i] = now;
            }
        }
        if v.result.is_some() && !a.had_result {
            a.had_result = true;
            a.result_time = now;
        }
        if v.result.is_none() {
            a.had_result = false;
        }
        // начало нашего хода — сбрасываем панель рейза
        let key = (v.hand_no, v.seq);
        if v.to_act == v.you && v.you >= 0 && key != self.last_turn_key {
            self.last_turn_key = key;
            self.raise_open = false;
            self.raise_to = v.min_raise_to;
        }
        if let Some(n) = &mut self.net {
            n.view = Some(v);
            n.recv_time = now;
        }
    }

    // ---------------------------------------------------------- общие элементы

    fn draw_background(&self, g: &G) {
        g.vgradient(0.0, 0.0, g.w, g.h, rgb(0x161d3d), rgb(0x04060d));
        for i in 0..8 {
            let k = i as f32 / 8.0;
            g.ellipse(g.w / 2.0, g.h * 0.5, g.w * 0.55 - k * 300.0, g.h * 0.55 - k * 180.0, alpha(rgb(0x2a3a7a), 0.025));
        }
    }

    fn text_field(&mut self, g: &G, x: f32, y: f32, w: f32, h: f32, field: Field, placeholder: &str) {
        let focused = self.field == field;
        g.rrect(x, y, w, h, 12.0, rgb(0x0a0e1c));
        g.rrect_lines(x, y, w, h, 12.0, 2.0, if focused { K_GOLD } else { rgb(0x34407a) });
        let value = match field {
            Field::Name => self.settings.name.clone(),
            Field::Ip => self.settings.last_ip.clone(),
            Field::None => String::new(),
        };
        if value.is_empty() && !focused {
            g.text(placeholder, x + 16.0, y + h / 2.0, 20.0, K_MUTED, Align::Left, false);
        } else {
            let caret = if focused && (get_time() * 2.0) as i64 % 2 == 0 { "|" } else { "" };
            g.text(&format!("{}{}", value, caret), x + 16.0, y + h / 2.0, 22.0, K_TEXT, Align::Left, true);
        }
        if g.clicked(x, y, w, h) {
            self.set_field(field);
        }
    }

    fn handle_typing(&mut self) {
        let mut chars = Vec::new();
        while let Some(c) = get_char_pressed() {
            chars.push(c);
        }
        let backspace = is_key_pressed(KeyCode::Backspace);
        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        let field = self.field;
        let target = match field {
            Field::Name => &mut self.settings.name,
            Field::Ip => &mut self.settings.last_ip,
            Field::None => return,
        };
        let max = if field == Field::Name { 14 } else { 40 };
        for c in chars {
            if c == '\u{8}' {
                target.pop();
            } else if !c.is_control() && target.chars().count() < max {
                if field == Field::Ip && !(c.is_ascii_alphanumeric() || c == '.' || c == ':' || c == '-') {
                    continue;
                }
                target.push(c);
            }
        }
        if backspace {
            target.pop();
        }
        if enter {
            self.set_field(Field::None);
        }
    }

    fn avatar(&self, g: &G, x: f32, y: f32, r: f32, avatar: u32, name: &str, dim: bool) {
        g.circle(x + 2.0, y + 4.0, r + 4.0, alpha(BLACK, 0.5));
        g.circle(x, y, r + 4.0, K_GOLD_DARK);
        let col = rgb(AVATAR_COLORS[avatar as usize % AVATAR_COLORS.len()]);
        g.circle(x, y, r, darken(col, 0.25));
        g.circle(x, y - r * 0.1, r * 0.86, col);
        let initial: String = name.chars().next().unwrap_or('?').to_uppercase().collect();
        g.text_shadow(&initial, x, y - r * 0.06, r * 0.82, WHITE, Align::Center, true);
        if dim {
            g.circle(x, y, r + 4.0, alpha(rgb(0x05070f), 0.55));
        }
    }

    // ---------------------------------------------------------- меню

    fn draw_menu(&mut self, g: &G) {
        // медленно вращающийся стол на фоне
        let t = get_time() as f32 * 0.12;
        let cam = Camera3D {
            position: vec3(t.sin() * 13.0, 7.5, t.cos() * 13.0),
            target: vec3(0.0, -0.5, 0.0),
            up: Vec3::Y,
            fovy: 40f32.to_radians(),
            z_near: 2.0,
            z_far: 60.0,
            ..Default::default()
        };
        set_camera(&cam);
        self.scene.draw_table();
        for i in 0..5 {
            self.scene.draw_card(board_world(i), 0.0, 1.0, Some((i * 9 + 12) as u8 % 52), 1.0, 0.0);
        }
        self.scene.draw_chips(POT_WORLD, 4000, 100);
        set_default_camera();
        g.rect(0.0, 0.0, g.w, g.h, alpha(rgb(0x04060d), 0.45));

        let cx = g.w / 2.0;
        g.strawberry(cx - 290.0, 74.0, 70.0, 1.0);
        g.text_shadow("STRAWBERRY POKER", cx + 30.0, 64.0, 46.0, WHITE, Align::Center, true);
        g.text("3D Texas Hold'em · играй с друзьями по сети", cx + 30.0, 106.0, 18.0, K_MUTED, Align::Center, false);

        let (pw, ph) = (560.0, 440.0);
        let (px, py) = (cx - pw / 2.0, 150.0);
        g.rrect(px, py, pw, ph, 24.0, alpha(K_PANEL, 0.92));
        g.rrect_lines(px, py, pw, ph, 24.0, 2.0, alpha(K_GOLD, 0.5));

        match self.screen {
            Screen::Menu => {
                g.text("Ваше имя", px + 40.0, py + 40.0, 17.0, K_MUTED, Align::Left, false);
                self.text_field(g, px + 40.0, py + 58.0, pw - 80.0, 56.0, Field::Name, "Имя");
                g.text("Аватар", px + 40.0, py + 148.0, 17.0, K_MUTED, Align::Left, false);
                for i in 0..10u32 {
                    let x = px + 64.0 + i as f32 * 48.0;
                    let y = py + 196.0;
                    if self.settings.avatar == i {
                        g.circle(x, y, 25.0, K_GOLD);
                    }
                    g.circle(x, y, 20.0, rgb(AVATAR_COLORS[i as usize]));
                    if g.clicked(x - 23.0, y - 23.0, 46.0, 46.0) {
                        self.settings.avatar = i;
                    }
                }
                if g.button(px + 40.0, py + 256.0, pw - 80.0, 66.0, "Создать стол", rgb(0x23915a), true) {
                    self.set_field(Field::None);
                    self.screen = Screen::Host;
                    self.message = None;
                }
                if g.button(px + 40.0, py + 340.0, pw - 80.0, 66.0, "Подключиться к другу", rgb(0x2a6fd6), true) {
                    self.set_field(Field::None);
                    self.screen = Screen::Join;
                    self.message = None;
                }
            }
            Screen::Host => {
                g.text("Новый стол", cx, py + 40.0, 26.0, K_GOLD, Align::Center, true);
                let (sb, bb) = BLINDS[self.settings.blinds];
                let rows = [
                    ("Блайнды", format!("{}/{}", fmt_chips(sb), fmt_chips(bb))),
                    ("Стартовый стек", fmt_chips(bb * STACK_BB[self.settings.stack])),
                    ("Ботов сразу", self.settings.bots.to_string()),
                ];
                for (k, (label, value)) in rows.iter().enumerate() {
                    let y = py + 86.0 + k as f32 * 72.0;
                    g.text(label, px + 40.0, y + 27.0, 19.0, K_TEXT, Align::Left, false);
                    let minus = g.button(px + pw - 270.0, y, 54.0, 54.0, "−", rgb(0x34407a), true);
                    g.text(value, px + pw - 146.0, y + 27.0, 21.0, K_GOLD, Align::Center, true);
                    let plus = g.button(px + pw - 94.0, y, 54.0, 54.0, "+", rgb(0x34407a), true);
                    let (val, max) = match k {
                        0 => (&mut self.settings.blinds, BLINDS.len() - 1),
                        1 => (&mut self.settings.stack, STACK_BB.len() - 1),
                        _ => (&mut self.settings.bots, MAX_SEATS - 1),
                    };
                    if minus && *val > 0 {
                        *val -= 1;
                    }
                    if plus && *val < max {
                        *val += 1;
                    }
                }
                g.text("Боты займут свободные места;", cx, py + 316.0, 15.0, K_MUTED, Align::Center, false);
                g.text("подключившиеся друзья сядут вместо них.", cx, py + 338.0, 15.0, K_MUTED, Align::Center, false);
                if g.button(px + 40.0, py + 362.0, 160.0, 60.0, "Назад", rgb(0x2b3560), true) {
                    self.screen = Screen::Menu;
                }
                if g.button(px + 216.0, py + 362.0, pw - 256.0, 60.0, "Открыть стол", rgb(0x23915a), true) {
                    self.host_game();
                }
            }
            Screen::Join => {
                g.text("Подключение", cx, py + 38.0, 26.0, K_GOLD, Align::Center, true);
                g.text("IP-адрес хоста (виден у него на экране)", px + 40.0, py + 78.0, 16.0, K_MUTED, Align::Left, false);
                self.text_field(g, px + 40.0, py + 96.0, pw - 80.0, 56.0, Field::Ip, "например 192.168.1.5");
                // своя цифровая клавиатура — на телефоне удобнее системной
                let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", ".", "0", "⌫"];
                let (kw, kh) = (100.0, 44.0);
                let kx0 = px + 40.0;
                for (k, key) in keys.iter().enumerate() {
                    let x = kx0 + (k % 3) as f32 * (kw + 8.0);
                    let y = py + 166.0 + (k / 3) as f32 * (kh + 6.0);
                    if g.button(x, y, kw, kh, key, rgb(0x283260), true) {
                        if *key == "⌫" {
                            self.settings.last_ip.pop();
                        } else if self.settings.last_ip.len() < 40 {
                            self.settings.last_ip.push_str(key);
                        }
                    }
                }
                let bx = kx0 + 3.0 * (kw + 8.0) + 8.0;
                let bw = px + pw - 40.0 - bx;
                if g.button(bx, py + 166.0, bw, 44.0, ": порт", rgb(0x283260), true) {
                    self.settings.last_ip.push(':');
                }
                if g.button(bx, py + 216.0, bw, 44.0, "Очистить", rgb(0x283260), true) {
                    self.settings.last_ip.clear();
                }
                if g.button(px + 40.0, py + 370.0, 160.0, 56.0, "Назад", rgb(0x2b3560), true) {
                    self.set_field(Field::None);
                    self.screen = Screen::Menu;
                }
                if g.button(px + 216.0, py + 370.0, pw - 256.0, 56.0, "Подключиться", rgb(0x2a6fd6), true) {
                    self.set_field(Field::None);
                    self.join_game();
                }
            }
            Screen::Table => {}
        }

        if let Some(m) = &self.message {
            let w = g.text_w(m, 17.0, true) + 40.0;
            g.rrect(cx - w / 2.0, py + ph + 16.0, w, 40.0, 20.0, alpha(rgb(0x7a1a28), 0.95));
            g.text(m, cx, py + ph + 36.0, 17.0, WHITE, Align::Center, true);
        }
        g.text(
            &format!("v{} · порт {}", env!("CARGO_PKG_VERSION"), DEFAULT_PORT),
            g.w - 14.0,
            g.h - 16.0,
            13.0,
            alpha(K_MUTED, 0.6),
            Align::Right,
            false,
        );
        // клик мимо полей снимает фокус
        if g.clicked(0.0, 0.0, g.w, g.h) {
            self.set_field(Field::None);
        }
    }

    // ---------------------------------------------------------- стол

    fn draw_table(&mut self, g: &G) {
        let now = get_time();
        let cam = Scene::camera();
        set_camera(&cam);
        self.scene.draw_table();
        let Some(net) = &self.net else { return };
        let Some(v) = net.view.clone() else {
            set_default_camera();
            let msg = if net.connected { "Садимся за стол…" } else { "Подключение…" };
            g.rect(0.0, 0.0, g.w, g.h, alpha(BLACK, 0.4));
            g.text_shadow(msg, g.w / 2.0, g.h / 2.0, 30.0, K_GOLD, Align::Center, true);
            if g.button(g.w / 2.0 - 90.0, g.h / 2.0 + 50.0, 180.0, 56.0, "Отмена", rgb(0x2b3560), true) {
                self.leave(None);
            }
            return;
        };
        let you = v.you;
        let a = &self.anim;
        let since_result = (now - a.result_time) as f32;
        let result = v.result.as_ref();
        let best: Vec<u8> = result
            .filter(|r| r.showdown && since_result > 0.6)
            .map(|r| r.best_cards.clone())
            .unwrap_or_default();
        let n_dealt = v.seats.iter().filter(|s| s.has_cards || s.in_hand).count().max(1);

        // ---- борд
        for (i, &code) in v.board.iter().enumerate() {
            let t0 = a.board_times.get(i).copied().unwrap_or(0.0);
            if now < t0 {
                continue;
            }
            let p = ((now - t0) / 0.4) as f32;
            let pos = DECK_WORLD.lerp(board_world(i), ease_out(p * 1.5));
            let hl = best.contains(&code);
            let lift = if hl { 0.12 } else { 0.0 };
            if hl {
                self.scene.draw_card_glow(pos, 1.0, lift);
            }
            self.scene.draw_card(pos, 0.0, 1.0, Some(code), ease_out(p), lift);
        }

        // ---- карты соперников, ставки, дилер
        for (seat, s) in v.seats.iter().enumerate() {
            if !s.occupied {
                continue;
            }
            let d = seat_disp(you, seat);
            if s.has_cards && (seat as i32 != you || you < 0) {
                let order = (seat + MAX_SEATS - v.dealer.max(0) as usize) % MAX_SEATS;
                for k in 0..2 {
                    let t0 = a.deal_time + ((k * n_dealt + order) as f64) * 0.08;
                    if now < t0 {
                        continue;
                    }
                    let prog = ease_out(((now - t0) / 0.3) as f32);
                    let target = hole_world(d) + vec3((k as f32 - 0.5) * 0.42, k as f32 * 0.004, 0.0);
                    let pos = DECK_WORLD.lerp(target, prog);
                    let yaw = (k as f32 - 0.5) * 0.18;
                    let code = s.cards.get(k).copied();
                    let flip = match code {
                        Some(_) => ((now - a.reveal_times[seat]) / 0.4) as f32,
                        None => 0.0,
                    };
                    let hl = code.map(|c| best.contains(&c)).unwrap_or(false);
                    let lift = if hl { 0.1 } else { 0.0 };
                    if hl {
                        self.scene.draw_card_glow(pos, 0.62, lift);
                    }
                    self.scene.draw_card(pos, yaw, 0.62, code, flip, lift);
                }
            }
            if s.bet > 0 {
                self.scene.draw_chips(bet_world(d), s.bet, v.bb);
            }
            if v.dealer == seat as i32 && v.hand_no > 0 {
                self.scene.draw_dealer(dealer_world(d));
            }
        }

        // ---- банк
        let collected = v.pot.saturating_sub(v.seats.iter().map(|s| s.bet).sum::<u64>());
        if let Some(r) = result {
            for w in &r.wins {
                let t = ease_out(since_result / 0.9);
                let to = bet_world(seat_disp(you, w.seat as usize));
                self.scene.draw_chips(POT_WORLD.lerp(to, t), w.amount, v.bb);
            }
        } else if collected > 0 {
            self.scene.draw_chips(POT_WORLD, collected, v.bb);
        }

        set_default_camera();
        self.draw_table_hud(g, &v, &cam, now);
    }

    fn draw_table_hud(&mut self, g: &G, v: &View, cam: &Camera3D, now: f64) {
        let you = v.you;
        let since_result = (now - self.anim.result_time) as f32;
        let result = v.result.as_ref();
        let recv = self.net.as_ref().map(|n| n.recv_time).unwrap_or(now);

        // ---- надписи банка и ставок
        if v.pot > 0 && result.is_none() {
            let p = Scene::project(cam, g, POT_WORLD + Vec3::Y * 0.2);
            let label = format!("Банк: {}", fmt_chips(v.pot));
            let w = g.text_w(&label, 19.0, true) + 30.0;
            g.rrect(p.x - w / 2.0, p.y - 60.0, w, 32.0, 16.0, alpha(BLACK, 0.55));
            g.text(&label, p.x, p.y - 44.0, 19.0, K_GOLD, Align::Center, true);
        }
        for (seat, s) in v.seats.iter().enumerate() {
            if s.occupied && s.bet > 0 {
                let p = Scene::project(cam, g, bet_world(seat_disp(you, seat)));
                let label = fmt_chips(s.bet);
                let w = g.text_w(&label, 15.0, true) + 16.0;
                g.rrect(p.x + 20.0, p.y - 12.0, w, 24.0, 12.0, alpha(BLACK, 0.6));
                g.text(&label, p.x + 28.0, p.y, 15.0, K_TEXT, Align::Left, true);
            }
        }

        // ---- места
        for seat in 0..MAX_SEATS {
            let d = seat_disp(you, seat);
            let mut pos = Scene::project(cam, g, seat_world(d));
            if d == 0 {
                pos.y = pos.y.min(g.h - 120.0);
            }
            pos.y = pos.y.max(78.0);
            if !v.seats[seat].occupied {
                g.circle(pos.x, pos.y, 40.0, alpha(BLACK, 0.35));
                g.ring(pos.x, pos.y, 40.0, 2.0, alpha(K_MUTED, 0.5));
                if v.host {
                    if g.button(pos.x - 50.0, pos.y - 18.0, 100.0, 36.0, "+ Бот", rgb(0x34407a), true) {
                        self.send(ClientMsg::new("add_bot"));
                    }
                } else {
                    g.text("Свободно", pos.x, pos.y, 14.0, K_MUTED, Align::Center, true);
                }
                continue;
            }
            self.draw_seat(g, v, seat, pos, now, recv, since_result);
        }

        // ---- результат
        if let Some(r) = result {
            if since_result > 0.5 {
                let a = ((since_result - 0.5) / 0.3).clamp(0.0, 1.0);
                let lines: Vec<&str> = r.text.lines().take(3).collect();
                let p = Scene::project(cam, g, vec3(0.0, 0.0, 1.5));
                let h = 18.0 + lines.len() as f32 * 26.0;
                let wmax = lines.iter().map(|l| g.text_w(l, 18.0, true)).fold(0.0, f32::max) + 40.0;
                g.rrect(p.x - wmax / 2.0, p.y - h / 2.0, wmax, h, 14.0, alpha(rgb(0x0b0f1e), 0.88 * a));
                g.rrect_lines(p.x - wmax / 2.0, p.y - h / 2.0, wmax, h, 14.0, 1.5, alpha(K_GOLD, 0.7 * a));
                for (k, l) in lines.iter().enumerate() {
                    g.text(l, p.x, p.y - h / 2.0 + 21.0 + k as f32 * 26.0, 18.0, alpha(K_GOLD, a), Align::Center, true);
                }
            }
        }

        // ---- свои карты крупно
        if you >= 0 {
            let me = &v.seats[you as usize];
            let mut pos = Scene::project(cam, g, seat_world(0));
            pos.y = pos.y.min(g.h - 120.0);
            if me.cards.len() == 2 && !me.folded {
                let n_dealt = v.seats.iter().filter(|s| s.has_cards || s.in_hand).count().max(1);
                let order = (you as usize + MAX_SEATS - v.dealer.max(0) as usize) % MAX_SEATS;
                let best: Vec<u8> = result
                    .filter(|r| r.showdown && since_result > 0.6)
                    .map(|r| r.best_cards.clone())
                    .unwrap_or_default();
                for (k, &code) in me.cards.iter().enumerate() {
                    let t0 = self.anim.deal_time + ((k * n_dealt + order) as f64) * 0.08;
                    if now < t0 {
                        continue;
                    }
                    let prog = ease_out(((now - t0) / 0.3) as f32);
                    let (w, h) = (84.0, 118.0);
                    let target = vec2(pos.x + 62.0 + k as f32 * 64.0, pos.y - 84.0);
                    let from = vec2(g.w / 2.0, g.h * 0.3);
                    let at = from.lerp(target, prog);
                    let flip = (((now - t0) - 0.3) / 0.3) as f32;
                    let hl = best.contains(&code);
                    if prog < 1.0 {
                        let k = 0.6 + 0.4 * prog;
                        g.card_back(at.x, at.y, w * k, h * k);
                    } else {
                        let lift = if hl { -10.0 } else { 0.0 };
                        g.card_flip(at.x, at.y + lift, w, h, code_card(code), flip.clamp(0.0, 1.0), false, hl);
                    }
                }
                // подсказка комбинации
                let mut cards: Vec<_> = me.cards.iter().map(|&c| code_card(c)).collect();
                let hint = if v.board.len() >= 3 {
                    cards.extend(v.board.iter().map(|&c| code_card(c)));
                    evaluate(&cards).describe()
                } else if cards[0].rank == cards[1].rank {
                    format!("Пара: {}", cards[0].rank_str())
                } else {
                    let hi = cards.iter().max_by_key(|c| c.rank).unwrap();
                    format!("Старшая карта: {}", hi.rank_str())
                };
                let hx = pos.x + 62.0 + 74.0;
                let w = g.text_w(&hint, 15.0, true) + 24.0;
                g.rrect(hx - w / 2.0, pos.y - 122.0, w, 28.0, 14.0, alpha(rgb(0x0b0f1e), 0.85));
                g.text(&hint, hx, pos.y - 108.0, 15.0, rgb(0x9fe8bf), Align::Center, true);
            }
        }

        self.draw_top_bar(g, v);
        self.draw_log(g, v);

        if !v.started {
            self.draw_waiting(g, v);
        } else {
            if !v.status.is_empty() && result.is_none() && v.to_act < 0 && !v.runout {
                g.text_shadow(&v.status, g.w / 2.0, g.h * 0.36, 24.0, K_GOLD, Align::Center, true);
            }
            if you >= 0 {
                let me = &v.seats[you as usize];
                let in_hand_now = me.in_hand && result.is_none();
                if me.chips == 0 && !in_hand_now {
                    let label = format!("Докупиться на {}", fmt_chips(v.stack));
                    if g.button(g.w - 300.0, g.h - 84.0, 280.0, 64.0, &label, rgb(0x23915a), true) {
                        self.send(ClientMsg::new("rebuy"));
                    }
                }
            } else {
                g.text_shadow("Вы зритель: свободных мест нет", g.w / 2.0, g.h - 40.0, 20.0, K_MUTED, Align::Center, true);
            }
            self.draw_controls(g, v);
        }

        if self.confirm_leave {
            self.draw_confirm_leave(g, v);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_seat(&mut self, g: &G, v: &View, seat: usize, pos: Vec2, now: f64, recv: f64, since_result: f32) {
        let s = &v.seats[seat];
        let result = v.result.as_ref();
        let is_turn = v.to_act == seat as i32 && result.is_none();
        let winner = result.and_then(|r| r.wins.iter().find(|w| w.seat as usize == seat));
        let dim = s.folded || (!s.in_hand && v.hand_no > 0 && v.started) || !s.connected;
        let r = 40.0;

        if winner.is_some() {
            let pulse = 0.5 + 0.5 * (now as f32 * 6.0).sin();
            g.circle(pos.x, pos.y, r + 14.0 + pulse * 4.0, alpha(K_GOLD, 0.25));
        }
        self.avatar(g, pos.x, pos.y, r, s.avatar, &s.name, dim);
        if winner.is_some() {
            g.ring(pos.x, pos.y, r + 4.0, 4.0, K_GOLD);
        } else if seat as i32 == v.you {
            g.ring(pos.x, pos.y, r + 4.0, 3.0, rgb(0x9fe8bf));
        }

        if is_turn {
            let total = v.turn_total_ms.max(1) as f64 / 1000.0;
            let left = v.turn_left_ms as f64 / 1000.0 - (now - recv);
            let frac = (left / total).clamp(0.0, 1.0) as f32;
            let c = if frac > 0.5 {
                mix(rgb(0xffd23f), rgb(0x3ddc84), (frac - 0.5) * 2.0)
            } else {
                mix(rgb(0xff4d4d), rgb(0xffd23f), frac * 2.0)
            };
            g.ring(pos.x, pos.y, r + 10.0, 6.0, alpha(BLACK, 0.5));
            g.arc(pos.x, pos.y, r + 10.0, 6.0, -90.0, 360.0 * frac, c);
        }

        // табличка
        let (pw, ph) = (150.0, 48.0);
        let px = pos.x - pw / 2.0;
        let py = pos.y + r - 6.0;
        g.rrect(px, py + 3.0, pw, ph, 12.0, alpha(BLACK, 0.45));
        g.rrect(px, py, pw, ph, 12.0, if is_turn { rgb(0x23305e) } else { rgb(0x111830) });
        g.rrect_lines(px, py, pw, ph, 12.0, 1.5, if is_turn { K_GOLD } else { alpha(rgb(0x4a5a9a), 0.8) });
        let mut name = s.name.clone();
        while g.text_w(&name, 15.0, true) > pw - 16.0 && name.chars().count() > 2 {
            name.pop();
        }
        g.text(&name, pos.x, py + 15.0, 15.0, if dim { K_MUTED } else { K_TEXT }, Align::Center, true);
        let chips_c = if s.all_in { K_ORANGE } else { K_GOLD };
        g.text(&fmt_chips(s.chips), pos.x, py + 34.0, 17.0, chips_c, Align::Center, true);

        if s.is_bot {
            g.rrect(pos.x + r - 18.0, pos.y + r - 30.0, 40.0, 18.0, 9.0, rgb(0x34407a));
            g.text("БОТ", pos.x + r + 2.0, pos.y + r - 21.0, 11.0, K_TEXT, Align::Center, true);
            if v.host && g.button(pos.x - r - 16.0, pos.y - r, 30.0, 30.0, "×", rgb(0x8a2b35), true) {
                let mut m = ClientMsg::new("kick");
                m.seat = seat as u32;
                self.send(m);
            }
        }
        if !s.connected {
            g.text_shadow("нет связи", pos.x, pos.y, 14.0, K_RED, Align::Center, true);
        }

        if let Some(w) = winner {
            let rise = (since_result * 22.0).min(26.0);
            let label = format!("+{}", fmt_chips(w.amount));
            g.text_shadow(&label, pos.x, pos.y - r - 18.0 - rise, 24.0, K_GOLD, Align::Center, true);
        } else if !s.last_action.is_empty() {
            let a = &s.last_action;
            let c = if a.starts_with("Фолд") {
                rgb(0x5a6077)
            } else if a.starts_with("Чек") {
                K_GREEN
            } else if a.starts_with("Колл") {
                K_BLUE
            } else if a.starts_with("Олл") {
                K_RED
            } else if a.starts_with("МБ") || a.starts_with("ББ") {
                rgb(0x3b456b)
            } else {
                K_ORANGE
            };
            let text = if a.starts_with("Олл") { "ОЛЛ-ИН".to_string() } else { a.clone() };
            let w = g.text_w(&text, 14.0, true) + 22.0;
            let by = pos.y - r - 30.0;
            g.rrect(pos.x - w / 2.0, by, w, 24.0, 12.0, c);
            g.tri(vec2(pos.x - 6.0, by + 23.0), vec2(pos.x + 6.0, by + 23.0), vec2(pos.x, by + 30.0), c);
            g.text(&text, pos.x, by + 12.0, 14.0, WHITE, Align::Center, true);
        }
    }

    fn draw_top_bar(&mut self, g: &G, v: &View) {
        if g.button(14.0, 12.0, 120.0, 42.0, "← Выйти", rgb(0x2b3560), true) {
            self.confirm_leave = true;
        }
        let mut info = format!("Блайнды {}/{}", fmt_chips(v.sb), fmt_chips(v.bb));
        if let Some(n) = &self.net {
            if n.server.is_some() {
                let ip = n.ips.first().cloned().unwrap_or_else(|| "?".into());
                let addr = if n.port == DEFAULT_PORT { ip } else { format!("{}:{}", ip, n.port) };
                info = format!("{}  ·  Ваш стол: {}", info, addr);
            }
        }
        let w = g.text_w(&info, 16.0, true) + 32.0;
        g.rrect(g.w - w - 14.0, 12.0, w, 42.0, 21.0, K_PANEL);
        g.text(&info, g.w - 30.0, 33.0, 16.0, K_GOLD, Align::Right, true);
    }

    fn draw_log(&self, g: &G, v: &View) {
        let (lx, lw, lh) = (14.0, 320.0, 146.0);
        let ly = g.h - lh - 14.0;
        g.rrect(lx, ly, lw, lh, 12.0, alpha(K_PANEL, 0.7));
        let lines: Vec<&String> = v.log.iter().rev().take(6).collect();
        for (k, l) in lines.iter().rev().enumerate() {
            let c = if l.starts_with('—') { K_GOLD } else { K_MUTED };
            let mut s = l.to_string();
            while g.text_w(&s, 13.0, false) > lw - 20.0 && s.chars().count() > 3 {
                s.pop();
            }
            g.text(&s, lx + 10.0, ly + 17.0 + k as f32 * 22.5, 13.0, c, Align::Left, false);
        }
    }

    fn draw_waiting(&mut self, g: &G, v: &View) {
        let (pw, ph) = (540.0, if v.host { 250.0 } else { 130.0 });
        let (px, py) = (g.w / 2.0 - pw / 2.0, g.h * 0.28 - 30.0);
        g.rrect(px, py, pw, ph, 20.0, alpha(K_PANEL, 0.94));
        g.rrect_lines(px, py, pw, ph, 20.0, 2.0, alpha(K_GOLD, 0.6));
        let seated = v.seats.iter().filter(|s| s.occupied).count();
        if v.host {
            g.text("Стол открыт!", g.w / 2.0, py + 32.0, 24.0, K_GOLD, Align::Center, true);
            let (ips, port) = self.net.as_ref().map(|n| (n.ips.clone(), n.port)).unwrap_or_default();
            let mut addr = if ips.is_empty() { "IP не определён".to_string() } else { ips.join("  или  ") };
            if port != DEFAULT_PORT {
                addr = format!("{}  (порт {})", addr, port);
            }
            g.text("Друзья вводят этот адрес:", g.w / 2.0, py + 68.0, 16.0, K_MUTED, Align::Center, false);
            g.text(&addr, g.w / 2.0, py + 102.0, 30.0, WHITE, Align::Center, true);
            let txt = format!("За столом: {} из {}", seated, MAX_SEATS);
            g.text(&txt, g.w / 2.0, py + 142.0, 16.0, K_MUTED, Align::Center, false);
            if g.button(px + 30.0, py + 170.0, 200.0, 56.0, "+ Бот", rgb(0x34407a), seated < MAX_SEATS) {
                self.send(ClientMsg::new("add_bot"));
            }
            if g.button(px + 246.0, py + 170.0, pw - 276.0, 56.0, "Начать игру", rgb(0x23915a), seated >= 2) {
                self.send(ClientMsg::new("start"));
            }
        } else {
            g.text("Вы за столом!", g.w / 2.0, py + 38.0, 24.0, K_GOLD, Align::Center, true);
            let txt = format!("Ждём, пока хост начнёт игру · игроков: {}", seated);
            g.text(&txt, g.w / 2.0, py + 86.0, 17.0, K_TEXT, Align::Center, false);
        }
    }

    fn draw_controls(&mut self, g: &G, v: &View) {
        if v.you < 0 || v.to_act != v.you || v.result.is_some() {
            return;
        }
        let me = &v.seats[v.you as usize];
        let to_call = v.to_call;
        let (lo, hi) = (v.min_raise_to, v.max_raise_to.max(v.min_raise_to));
        let bet_word = if v.current_bet == 0 { "Бет" } else { "Рейз" };
        self.raise_to = self.raise_to.clamp(lo, hi);
        let (bw, bh, gap) = (132.0, 66.0, 8.0);
        let bx = g.w - 3.0 * bw - 2.0 * gap - 14.0;
        let by = g.h - bh - 14.0;

        if self.raise_open && v.can_raise {
            let (px, py, pw, ph) = (bx, by - 108.0, 3.0 * bw + 2.0 * gap, 98.0);
            g.rrect(px, py, pw, ph, 14.0, K_PANEL);
            g.rrect_lines(px, py, pw, ph, 14.0, 1.5, alpha(K_GOLD, 0.4));
            let pot = v.pot;
            let quick = [
                ("Мин", lo),
                ("½ банка", me.bet + to_call + (pot + to_call) / 2),
                ("Банк", me.bet + to_call + pot + to_call),
                ("Олл-ин", hi),
            ];
            let qw = (pw - 20.0 - 3.0 * 6.0) / 4.0;
            for (k, (label, val)) in quick.iter().enumerate() {
                let x = px + 10.0 + k as f32 * (qw + 6.0);
                if g.button(x, py + 10.0, qw, 34.0, label, rgb(0x34407a), true) {
                    self.raise_to = (*val).clamp(lo, hi);
                }
            }
            let (sx, sy, sw) = (px + 22.0, py + 72.0, pw - 44.0);
            g.rrect(sx, sy - 5.0, sw, 10.0, 5.0, rgb(0x060814));
            let frac = if hi > lo { (self.raise_to - lo) as f32 / (hi - lo) as f32 } else { 1.0 };
            g.rrect(sx, sy - 5.0, sw * frac, 10.0, 5.0, K_ORANGE);
            let kx = sx + sw * frac;
            g.circle(kx, sy, 15.0, WHITE);
            g.circle(kx, sy, 11.0, K_ORANGE);
            if g.clicked(sx - 16.0, sy - 22.0, sw + 32.0, 44.0) {
                self.dragging = true;
            }
            if !is_mouse_button_down(MouseButton::Left) {
                self.dragging = false;
            }
            if self.dragging && hi > lo {
                let (mx, _) = g.mouse();
                let f = ((mx - sx) / sw).clamp(0.0, 1.0);
                let unit = (v.bb / 2).max(1);
                let val = lo + ((hi - lo) as f32 * f) as u64;
                self.raise_to = if f >= 0.995 { hi } else { ((val / unit) * unit).clamp(lo, hi) };
            }
            let wheel = mouse_wheel().1;
            if wheel != 0.0 {
                let step = v.bb as i64 * if wheel > 0.0 { 1 } else { -1 };
                self.raise_to = ((self.raise_to as i64 + step).max(0) as u64).clamp(lo, hi);
            }
        }

        let call_label = if to_call == 0 {
            "Чек".to_string()
        } else if to_call >= me.chips {
            format!("Олл-ин {}", fmt_chips(to_call))
        } else {
            format!("Колл {}", fmt_chips(to_call))
        };
        let raise_label = if !self.raise_open {
            bet_word.to_string()
        } else if self.raise_to >= hi {
            format!("Олл-ин {}", fmt_chips(hi))
        } else {
            format!("{} {}", bet_word, fmt_chips(self.raise_to))
        };

        if g.button(bx, by, bw, bh, "Фолд", rgb(0xc0303c), true) || is_key_pressed(KeyCode::F) {
            // фолд без ставки превращаем в чек
            self.act(if to_call == 0 { 1 } else { 0 }, 0);
            return;
        }
        let call_c = if to_call == 0 { rgb(0x23915a) } else { rgb(0x2a6fd6) };
        if g.button(bx + bw + gap, by, bw, bh, &call_label, call_c, true) || is_key_pressed(KeyCode::C) {
            self.act(if to_call == 0 { 1 } else { 2 }, 0);
            return;
        }
        let raise = g.button(bx + 2.0 * (bw + gap), by, bw, bh, &raise_label, rgb(0xe08a1a), v.can_raise);
        if raise || (v.can_raise && is_key_pressed(KeyCode::R)) {
            if self.raise_open {
                let amt = self.raise_to;
                self.act(3, amt);
            } else {
                self.raise_open = true;
                self.raise_to = lo;
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            self.raise_open = false;
        }
    }

    fn draw_confirm_leave(&mut self, g: &G, v: &View) {
        g.rect(0.0, 0.0, g.w, g.h, alpha(BLACK, 0.6));
        let (w, h) = (480.0, 220.0);
        let (x, y) = (g.w / 2.0 - w / 2.0, g.h / 2.0 - h / 2.0);
        g.rrect(x, y, w, h, 20.0, rgb(0x111830));
        g.rrect_lines(x, y, w, h, 20.0, 2.0, K_GOLD);
        g.text("Покинуть стол?", g.w / 2.0, y + 44.0, 26.0, K_GOLD, Align::Center, true);
        let msg = if v.host { "Вы хост — стол закроется для всех" } else { "Ваше место освободится" };
        g.text(msg, g.w / 2.0, y + 92.0, 17.0, K_TEXT, Align::Center, false);
        let stay = g.button(x + 30.0, y + 134.0, 195.0, 58.0, "Остаться", rgb(0x2b3560), true);
        let go = g.button(x + w - 225.0, y + 134.0, 195.0, 58.0, "Выйти", rgb(0xc0303c), true);
        g.eat_click();
        if stay {
            self.confirm_leave = false;
        } else if go {
            self.confirm_leave = false;
            self.leave(None);
        }
    }

    fn frame(&mut self, g: &G) {
        self.handle_typing();
        // очищает и цвет, и буфер глубины — без этого 3D не рисуется
        clear_background(rgb(0x04060d));
        self.draw_background(g);
        if self.screen == Screen::Table {
            self.poll();
        }
        if self.screen == Screen::Table {
            self.draw_table(g);
        } else {
            self.draw_menu(g);
        }
    }
}

fn window_conf() -> macroquad::conf::Conf {
    // STRAWBERRY_WINDOW=2400x1080 — проверить вид на другом экране
    let (w, h) = std::env::var("STRAWBERRY_WINDOW")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or((1280, 720));
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Strawberry Poker".to_string(),
            window_width: w,
            window_height: h,
            high_dpi: true,
            // на Android не все GPU отдают MSAA-конфигурацию, а miniquad без неё падает
            sample_count: if cfg!(target_os = "android") { 1 } else { 4 },
            window_resizable: true,
            ..Default::default()
        },
        // меши стола крупные — увеличиваем буфер одного вызова отрисовки
        draw_call_vertex_capacity: 40_000,
        draw_call_index_capacity: 80_000,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/DejaVuSans.ttf")).unwrap();
    let bold = load_ttf_font_from_bytes(include_bytes!("../assets/DejaVuSans-Bold.ttf")).unwrap();
    let scene = Scene::new(&font, &bold);
    let mut app = App::new(scene);

    // Отладка: STRAWBERRY_SHOT=файл.png:секунды[:host|hostsetup|joinsetup|join=адрес]
    let mut shot: Option<(String, f64)> = None;
    if let Ok(spec) = std::env::var("STRAWBERRY_SHOT") {
        let parts: Vec<&str> = spec.splitn(3, ':').collect();
        let secs: f64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(2.0);
        match parts.get(2).copied() {
            Some("host") => app.host_game(),
            Some(other) if other.starts_with("join=") => {
                app.settings.last_ip = other[5..].to_string();
                app.join_game();
            }
            Some("hostsetup") => app.screen = Screen::Host,
            Some("joinsetup") => app.screen = Screen::Join,
            _ => {}
        }
        shot = Some((parts[0].to_string(), secs));
    }
    let autostart = std::env::var("STRAWBERRY_AUTOSTART").is_ok();

    loop {
        let g = G::new(&font, &bold);
        if autostart {
            if let Some(v) = app.net.as_ref().and_then(|n| n.view.as_ref()) {
                if v.host && !v.started && v.seats.iter().filter(|s| s.occupied).count() >= 2 {
                    app.send(ClientMsg::new("start"));
                }
            }
        }
        if std::env::var("STRAWBERRY_AUTOPLAY").is_ok() {
            if let Some(v) = app.net.as_ref().and_then(|n| n.view.as_ref()) {
                if v.you >= 0 && v.to_act == v.you && v.result.is_none() && (get_time() as i64) % 2 == 0 {
                    let mut m = ClientMsg::new("act");
                    m.kind = 2;
                    app.send(m);
                }
            }
        }
        app.frame(&g);
        if let Some((path, secs)) = &shot {
            if get_time() > *secs {
                get_screen_data().export_png(path);
                break;
            }
        }
        next_frame().await
    }
}
