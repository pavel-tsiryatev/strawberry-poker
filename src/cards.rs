//! Карты, колода и генератор случайных чисел.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];

    pub fn symbol(self) -> &'static str {
        match self {
            Suit::Spades => "♠",
            Suit::Hearts => "♥",
            Suit::Diamonds => "♦",
            Suit::Clubs => "♣",
        }
    }

    pub fn is_red(self) -> bool {
        matches!(self, Suit::Hearts | Suit::Diamonds)
    }
}

/// Ранг карты: 2..=14 (14 = туз).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Card {
    pub rank: u8,
    pub suit: Suit,
}

impl Card {
    pub fn new(rank: u8, suit: Suit) -> Self {
        debug_assert!((2..=14).contains(&rank));
        Card { rank, suit }
    }

    pub fn rank_str(self) -> &'static str {
        rank_name(self.rank)
    }
}

pub fn rank_name(rank: u8) -> &'static str {
    match rank {
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        10 => "10",
        11 => "J",
        12 => "Q",
        13 => "K",
        14 => "A",
        _ => "?",
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank_str(), self.suit.symbol())
    }
}

/// Простой и быстрый ГСЧ (xorshift64*), чтобы не тянуть лишние зависимости.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1) ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        Rng::new(nanos)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Число в диапазоне 0..n
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// Число с плавающей точкой в [0, 1)
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

pub fn full_deck() -> Vec<Card> {
    let mut v = Vec::with_capacity(52);
    for suit in Suit::ALL {
        for rank in 2..=14 {
            v.push(Card::new(rank, suit));
        }
    }
    v
}

pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn shuffled(rng: &mut Rng) -> Self {
        let mut cards = full_deck();
        rng.shuffle(&mut cards);
        Deck { cards }
    }

    pub fn draw(&mut self) -> Card {
        self.cards.pop().expect("колода пуста")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_has_52_unique_cards() {
        let mut rng = Rng::new(7);
        let mut d = Deck::shuffled(&mut rng);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..52 {
            assert!(seen.insert(d.draw()));
        }
    }
}
