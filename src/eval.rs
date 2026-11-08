//! Оценка покерных комбинаций.

use crate::cards::{Card, rank_name};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Category {
    HighCard = 0,
    Pair,
    TwoPair,
    Trips,
    Straight,
    Flush,
    FullHouse,
    Quads,
    StraightFlush,
}

impl Category {
    pub fn name_ru(self) -> &'static str {
        match self {
            Category::HighCard => "Старшая карта",
            Category::Pair => "Пара",
            Category::TwoPair => "Две пары",
            Category::Trips => "Сет",
            Category::Straight => "Стрит",
            Category::Flush => "Флеш",
            Category::FullHouse => "Фулл-хаус",
            Category::Quads => "Каре",
            Category::StraightFlush => "Стрит-флеш",
        }
    }
}

/// Сила руки: чем больше значение, тем сильнее рука.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct HandValue(pub u32);

impl HandValue {
    fn make(cat: Category, kickers: &[u8]) -> Self {
        let mut v = (cat as u32) << 20;
        for (i, &k) in kickers.iter().take(5).enumerate() {
            v |= (k as u32) << (16 - 4 * i);
        }
        HandValue(v)
    }

    pub fn category(self) -> Category {
        match self.0 >> 20 {
            0 => Category::HighCard,
            1 => Category::Pair,
            2 => Category::TwoPair,
            3 => Category::Trips,
            4 => Category::Straight,
            5 => Category::Flush,
            6 => Category::FullHouse,
            7 => Category::Quads,
            _ => Category::StraightFlush,
        }
    }

    fn kicker(self, i: usize) -> u8 {
        ((self.0 >> (16 - 4 * i)) & 0xF) as u8
    }

    /// Человекочитаемое описание, например «Пара — K» или «Стрит до 9».
    pub fn describe(self) -> String {
        let c = self.category();
        let k0 = rank_name(self.kicker(0));
        let k1 = rank_name(self.kicker(1));
        match c {
            Category::HighCard => format!("{}: {}", c.name_ru(), k0),
            Category::Pair => format!("Пара: {}", k0),
            Category::TwoPair => format!("Две пары: {} и {}", k0, k1),
            Category::Trips => format!("Сет: {}", k0),
            Category::Straight => format!("Стрит до {}", k0),
            Category::Flush => format!("Флеш до {}", k0),
            Category::FullHouse => format!("Фулл-хаус: {} на {}", k0, k1),
            Category::Quads => format!("Каре: {}", k0),
            Category::StraightFlush => {
                if self.kicker(0) == 14 {
                    "Роял-флеш!".to_string()
                } else {
                    format!("Стрит-флеш до {}", k0)
                }
            }
        }
    }
}

/// Находит старшую карту стрита в битовой маске рангов (с учётом A-2-3-4-5).
fn straight_high(mask: u16) -> Option<u8> {
    let mut m = mask;
    if m & (1 << 14) != 0 {
        m |= 1 << 1; // туз как единица
    }
    for high in (5..=14).rev() {
        let need = 0b11111u16 << (high - 4);
        if m & need == need {
            return Some(high as u8);
        }
    }
    None
}

/// Оценивает лучшую 5-карточную комбинацию из 5..=7 карт.
pub fn evaluate(cards: &[Card]) -> HandValue {
    debug_assert!(cards.len() >= 5 && cards.len() <= 7);
    let mut counts = [0u8; 15];
    let mut suit_masks = [0u16; 4];
    let mut mask = 0u16;
    for c in cards {
        counts[c.rank as usize] += 1;
        suit_masks[c.suit as usize] |= 1 << c.rank;
        mask |= 1 << c.rank;
    }

    // Стрит-флеш и флеш
    let mut flush: Option<u16> = None;
    for &sm in &suit_masks {
        if sm.count_ones() >= 5 {
            if let Some(h) = straight_high(sm) {
                return HandValue::make(Category::StraightFlush, &[h]);
            }
            flush = Some(sm);
        }
    }

    // Группируем ранги по количеству
    let mut quads = Vec::new();
    let mut trips = Vec::new();
    let mut pairs = Vec::new();
    let mut singles = Vec::new();
    for r in (2..=14u8).rev() {
        match counts[r as usize] {
            4 => quads.push(r),
            3 => trips.push(r),
            2 => pairs.push(r),
            1 => singles.push(r),
            _ => {}
        }
    }

    let best_kickers = |exclude: &[u8], n: usize| -> Vec<u8> {
        (2..=14u8)
            .rev()
            .filter(|r| counts[*r as usize] > 0 && !exclude.contains(r))
            .take(n)
            .collect()
    };

    if let Some(&q) = quads.first() {
        let k = best_kickers(&[q], 1);
        return HandValue::make(Category::Quads, &[q, k[0]]);
    }

    if let Some(&t) = trips.first() {
        // Пара для фулл-хауса: второй сет или старшая пара
        let pair = trips
            .get(1)
            .copied()
            .into_iter()
            .chain(pairs.first().copied())
            .max();
        if let Some(p) = pair {
            return HandValue::make(Category::FullHouse, &[t, p]);
        }
    }

    if let Some(sm) = flush {
        let ranks: Vec<u8> = (2..=14u8).rev().filter(|r| sm & (1 << r) != 0).take(5).collect();
        return HandValue::make(Category::Flush, &ranks);
    }

    if let Some(h) = straight_high(mask) {
        return HandValue::make(Category::Straight, &[h]);
    }

    if let Some(&t) = trips.first() {
        let k = best_kickers(&[t], 2);
        return HandValue::make(Category::Trips, &[t, k[0], k[1]]);
    }

    if pairs.len() >= 2 {
        let (p1, p2) = (pairs[0], pairs[1]);
        let k = best_kickers(&[p1, p2], 1);
        return HandValue::make(Category::TwoPair, &[p1, p2, k[0]]);
    }

    if let Some(&p) = pairs.first() {
        let k = best_kickers(&[p], 3);
        return HandValue::make(Category::Pair, &[p, k[0], k[1], k[2]]);
    }

    let _ = singles;
    let k = best_kickers(&[], 5);
    HandValue::make(Category::HighCard, &k)
}

/// Какие из переданных карт образуют лучшую комбинацию (для подсветки).
pub fn best_five(cards: &[Card]) -> Vec<Card> {
    let target = evaluate(cards);
    let n = cards.len();
    if n <= 5 {
        return cards.to_vec();
    }
    // перебираем все 5-карточные подмножества (не более 21)
    let mut idx = [0usize; 5];
    fn rec(
        cards: &[Card],
        start: usize,
        depth: usize,
        idx: &mut [usize; 5],
        target: HandValue,
    ) -> Option<Vec<Card>> {
        if depth == 5 {
            let hand: Vec<Card> = idx.iter().map(|&i| cards[i]).collect();
            return (evaluate(&hand) == target).then_some(hand);
        }
        for i in start..cards.len() {
            idx[depth] = i;
            if let Some(h) = rec(cards, i + 1, depth + 1, idx, target) {
                return Some(h);
            }
        }
        None
    }
    rec(cards, 0, 0, &mut idx, target).unwrap_or_else(|| cards[..5].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Suit::*;
    use crate::cards::{Card, Suit};

    fn parse(s: &str) -> Vec<Card> {
        s.split_whitespace()
            .map(|t| {
                let (r, su) = t.split_at(t.len() - 1);
                let rank = match r {
                    "A" => 14,
                    "K" => 13,
                    "Q" => 12,
                    "J" => 11,
                    "T" => 10,
                    n => n.parse().unwrap(),
                };
                let suit: Suit = match su {
                    "s" => Spades,
                    "h" => Hearts,
                    "d" => Diamonds,
                    _ => Clubs,
                };
                Card::new(rank, suit)
            })
            .collect()
    }

    fn cat(s: &str) -> Category {
        evaluate(&parse(s)).category()
    }

    #[test]
    fn categories() {
        assert_eq!(cat("As Ks Qs Js Ts 2d 3c"), Category::StraightFlush);
        assert_eq!(cat("9h 9d 9s 9c 2d 3c 4h"), Category::Quads);
        assert_eq!(cat("9h 9d 9s 2c 2d 3c 4h"), Category::FullHouse);
        assert_eq!(cat("9h 9d 9s 2c 2d 2h 4h"), Category::FullHouse);
        assert_eq!(cat("2h 7h 9h Jh Kh 2d 3c"), Category::Flush);
        assert_eq!(cat("Ah 2d 3s 4c 5d Kc Kd"), Category::Straight);
        assert_eq!(cat("9h 9d 9s 2c 5d Kc 4h"), Category::Trips);
        assert_eq!(cat("9h 9d 5s 5c 2d Kc 4h"), Category::TwoPair);
        assert_eq!(cat("9h 9d 5s 7c 2d Kc 4h"), Category::Pair);
        assert_eq!(cat("9h Jd 5s 7c 2d Kc 4h"), Category::HighCard);
    }

    #[test]
    fn ordering() {
        let wheel = evaluate(&parse("Ah 2d 3s 4c 5d"));
        let six = evaluate(&parse("6h 2d 3s 4c 5d"));
        assert!(six > wheel);
        let ak = evaluate(&parse("Ah Ad Ks 4c 5d"));
        let aq = evaluate(&parse("Ah Ad Qs 4c 5d"));
        assert!(ak > aq);
        let fh1 = evaluate(&parse("Kh Kd Ks 2c 2d"));
        let fh2 = evaluate(&parse("Qh Qd Qs Ac Ad"));
        assert!(fh1 > fh2);
        // три пары: берётся лучшие две и лучший кикер
        let a = evaluate(&parse("Kh Kd Qs Qc 2d 2h Ac"));
        let b = evaluate(&parse("Kh Kd Qs Qc 3d 3h Jc"));
        assert!(a > b);
    }

    #[test]
    fn describe_royal() {
        let v = evaluate(&parse("As Ks Qs Js Ts"));
        assert_eq!(v.describe(), "Роял-флеш!");
    }

    #[test]
    fn best_five_picks_flush() {
        let cards = parse("2h 7h 9h Jh Kh 2d 3c");
        let five = best_five(&cards);
        assert!(five.iter().all(|c| c.suit == Hearts));
    }
}
