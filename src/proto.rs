//! Сетевой протокол: JSON-сообщения, по одному на строку, поверх TCP.

use nanoserde::{DeJson, SerJson};

use crate::cards::{Card, Suit};

pub const DEFAULT_PORT: u16 = 47474;
pub const MAX_SEATS: usize = 6;
pub const PROTO_VERSION: u32 = 1;

pub fn card_code(c: Card) -> u8 {
    (c.rank - 2) * 4 + c.suit as u8
}

pub fn code_card(code: u8) -> Card {
    let suit = Suit::ALL[(code % 4) as usize];
    Card::new(code / 4 + 2, suit)
}

/// Сообщение клиента серверу.
#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct ClientMsg {
    /// hello | act | add_bot | kick | start | rebuy | ping
    pub t: String,
    #[nserde(default)]
    pub name: String,
    #[nserde(default)]
    pub avatar: u32,
    #[nserde(default)]
    pub key: String,
    #[nserde(default)]
    pub version: u32,
    /// 0 фолд, 1 чек, 2 колл, 3 рейз до amount
    #[nserde(default)]
    pub kind: u32,
    #[nserde(default)]
    pub amount: u64,
    #[nserde(default)]
    pub seat: u32,
}

impl ClientMsg {
    pub fn new(t: &str) -> Self {
        ClientMsg { t: t.to_string(), ..Default::default() }
    }
}

#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct SeatView {
    pub occupied: bool,
    pub name: String,
    pub avatar: u32,
    pub chips: u64,
    pub bet: u64,
    pub folded: bool,
    pub all_in: bool,
    pub in_hand: bool,
    pub is_bot: bool,
    pub connected: bool,
    pub has_cards: bool,
    /// Открытые карты (свои или на вскрытии)
    pub cards: Vec<u8>,
    pub last_action: String,
}

#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct WinView {
    pub seat: u32,
    pub amount: u64,
    pub hand: String,
}

#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct ResultView {
    pub wins: Vec<WinView>,
    pub showdown: bool,
    pub best_cards: Vec<u8>,
    pub text: String,
}

/// Состояние стола глазами конкретного игрока.
#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct View {
    /// Номер места игрока, -1 — зритель
    pub you: i32,
    pub host: bool,
    pub started: bool,
    pub seats: Vec<SeatView>,
    pub board: Vec<u8>,
    pub pot: u64,
    pub dealer: i32,
    pub to_act: i32,
    pub turn_left_ms: u32,
    pub turn_total_ms: u32,
    pub current_bet: u64,
    pub to_call: u64,
    pub min_raise_to: u64,
    pub max_raise_to: u64,
    pub can_raise: bool,
    pub hand_no: u64,
    pub seq: u64,
    pub runout: bool,
    pub result: Option<ResultView>,
    pub log: Vec<String>,
    pub sb: u64,
    pub bb: u64,
    pub stack: u64,
    pub status: String,
}

/// Сообщение сервера клиенту.
#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct ServerMsg {
    /// view | error | info
    pub t: String,
    #[nserde(default)]
    pub view: Option<View>,
    #[nserde(default)]
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::full_deck;

    #[test]
    fn card_codes_roundtrip() {
        for c in full_deck() {
            assert_eq!(code_card(card_code(c)), c);
        }
    }

    #[test]
    fn json_roundtrip() {
        let mut v = View::default();
        v.seats.push(SeatView { name: "Тест \"кавычки\"".into(), cards: vec![1, 51], ..Default::default() });
        v.result = Some(ResultView { text: "a\nb".into(), ..Default::default() });
        let m = ServerMsg { t: "view".into(), view: Some(v), text: String::new() };
        let s = m.serialize_json();
        assert!(!s.contains('\n'));
        let back = ServerMsg::deserialize_json(&s).unwrap();
        let v = back.view.unwrap();
        assert_eq!(v.seats[0].name, "Тест \"кавычки\"");
        assert_eq!(v.result.unwrap().text, "a\nb");
        let c = ClientMsg::deserialize_json(r#"{"t":"ping"}"#).unwrap();
        assert_eq!(c.t, "ping");
    }
}
