//! Логика стола Texas Hold'em: блайнды, торговля, сайд-поты, вскрытие.

use crate::cards::{Card, Deck, Rng};
use crate::eval::{HandValue, best_five, evaluate};
use crate::fmt_chips;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Street {
    PreFlop,
    Flop,
    Turn,
    River,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Fold,
    Check,
    Call,
    /// Поднять общую ставку игрока на улице до указанной суммы.
    RaiseTo(u64),
}

/// Характер бота.
#[derive(Clone, Copy, Debug)]
pub struct BotStyle {
    /// 0 — играет всё подряд, 1 — очень тайтовый
    pub tight: f32,
    /// 0 — пассивный, 1 — очень агрессивный
    pub aggr: f32,
    /// вероятность блефа
    pub bluff: f32,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub name: String,
    pub chips: u64,
    pub hole: Vec<Card>,
    /// Ставка на текущей улице
    pub bet: u64,
    /// Всего вложено в банк в этой раздаче
    pub total: u64,
    pub folded: bool,
    pub all_in: bool,
    /// Участвует ли в текущей раздаче
    pub in_hand: bool,
    pub acted: bool,
    pub is_human: bool,
    pub style: BotStyle,
    pub avatar: usize,
    pub last_action: Option<String>,
    /// Место занято (пустые места тоже хранятся в списке игроков)
    pub seated: bool,
}

impl Player {
    pub fn new(name: &str, chips: u64, is_human: bool, style: BotStyle, avatar: usize) -> Self {
        Player {
            name: name.to_string(),
            chips,
            hole: Vec::new(),
            bet: 0,
            total: 0,
            folded: false,
            all_in: false,
            in_hand: false,
            acted: false,
            is_human,
            style,
            avatar,
            last_action: None,
            seated: true,
        }
    }

    /// Может ли ещё делать ходы в этой раздаче.
    pub fn can_act(&self) -> bool {
        self.in_hand && !self.folded && !self.all_in
    }

    pub fn contesting(&self) -> bool {
        self.in_hand && !self.folded
    }
}

#[derive(Clone, Debug)]
pub struct Win {
    pub seat: usize,
    pub amount: u64,
    pub hand: Option<HandValue>,
}

#[derive(Clone, Debug)]
pub struct HandResult {
    pub wins: Vec<Win>,
    /// Было ли вскрытие карт
    pub showdown: bool,
    /// Карты лучшей комбинации главного победителя (для подсветки)
    pub best_cards: Vec<Card>,
    pub text: String,
}

pub struct Game {
    pub players: Vec<Player>,
    pub board: Vec<Card>,
    pub dealer: usize,
    pub sb: u64,
    pub bb: u64,
    pub street: Street,
    pub to_act: Option<usize>,
    pub current_bet: u64,
    pub min_raise: u64,
    /// Фишки, уже собранные в банк с прошлых улиц
    pub pot: u64,
    pub result: Option<HandResult>,
    pub hand_no: u64,
    pub log: Vec<String>,
    /// Счётчик событий — растёт при каждом действии (для UI)
    pub seq: u64,
    deck: Deck,
    pub rng: Rng,
}

impl Game {
    pub fn new(players: Vec<Player>, sb: u64, bb: u64, mut rng: Rng) -> Self {
        let n = players.len();
        let dealer = rng.below(n);
        let deck = Deck::shuffled(&mut rng);
        Game {
            players,
            board: Vec::new(),
            dealer,
            sb,
            bb,
            street: Street::PreFlop,
            to_act: None,
            current_bet: 0,
            min_raise: bb,
            pot: 0,
            result: None,
            hand_no: 0,
            log: Vec::new(),
            seq: 0,
            deck,
            rng,
        }
    }

    fn push_log(&mut self, s: String) {
        self.seq += 1;
        self.log.push(s);
        if self.log.len() > 50 {
            self.log.remove(0);
        }
    }

    /// Следующее место после `from`, удовлетворяющее условию.
    fn next_seat(&self, from: usize, pred: impl Fn(&Player) -> bool) -> Option<usize> {
        let n = self.players.len();
        (1..=n).map(|i| (from + i) % n).find(|&i| pred(&self.players[i]))
    }

    pub fn total_pot(&self) -> u64 {
        self.pot + self.players.iter().map(|p| p.bet).sum::<u64>()
    }

    pub fn active_count(&self) -> usize {
        self.players.iter().filter(|p| p.seated && p.chips > 0).count()
    }

    pub fn hand_in_progress(&self) -> bool {
        self.result.is_none() && self.players.iter().any(|p| p.in_hand)
    }

    /// Раздача окончена и нужно доразложить борд без торговли (все в олл-ине).
    pub fn needs_runout(&self) -> bool {
        self.result.is_none() && self.to_act.is_none() && self.players.iter().any(|p| p.in_hand)
    }

    /// Начинает новую раздачу. Возвращает false, если играть некому.
    pub fn start_hand(&mut self) -> bool {
        if self.active_count() < 2 {
            return false;
        }
        self.hand_no += 1;
        self.result = None;
        self.board.clear();
        self.pot = 0;
        self.street = Street::PreFlop;
        self.deck = Deck::shuffled(&mut self.rng);
        for p in &mut self.players {
            p.hole.clear();
            p.bet = 0;
            p.total = 0;
            p.folded = false;
            p.all_in = false;
            p.acted = false;
            p.last_action = None;
            p.in_hand = p.seated && p.chips > 0;
        }

        self.dealer = self.next_seat(self.dealer, |p| p.in_hand).unwrap();
        let heads_up = self.players.iter().filter(|p| p.in_hand).count() == 2;
        let sb_seat = if heads_up {
            self.dealer
        } else {
            self.next_seat(self.dealer, |p| p.in_hand).unwrap()
        };
        let bb_seat = self.next_seat(sb_seat, |p| p.in_hand).unwrap();

        self.push_log(format!("— Раздача #{} —", self.hand_no));
        self.post(sb_seat, self.sb, "МБ");
        self.post(bb_seat, self.bb, "ББ");
        self.current_bet = self.bb;
        self.min_raise = self.bb;

        // Раздаём по две карты, начиная с малого блайнда
        for _ in 0..2 {
            let mut seat = sb_seat;
            loop {
                let c = self.deck.draw();
                self.players[seat].hole.push(c);
                seat = self.next_seat(seat, |p| p.in_hand).unwrap();
                if seat == sb_seat {
                    break;
                }
            }
        }

        self.to_act = None;
        let first = self.next_seat(bb_seat, |p| p.can_act());
        if self.round_complete() {
            self.to_act = None;
        } else {
            self.to_act = first;
        }
        true
    }

    fn post(&mut self, seat: usize, amount: u64, label: &str) {
        let p = &mut self.players[seat];
        let a = amount.min(p.chips);
        p.chips -= a;
        p.bet += a;
        p.total += a;
        if p.chips == 0 {
            p.all_in = true;
        }
        p.last_action = Some(format!("{} {}", label, fmt_chips(a)));
    }

    pub fn to_call(&self, seat: usize) -> u64 {
        let p = &self.players[seat];
        self.current_bet.saturating_sub(p.bet).min(p.chips)
    }

    /// Минимальная сумма «рейз до».
    pub fn min_raise_to(&self, seat: usize) -> u64 {
        let p = &self.players[seat];
        (self.current_bet + self.min_raise).min(p.bet + p.chips)
    }

    pub fn max_raise_to(&self, seat: usize) -> u64 {
        let p = &self.players[seat];
        p.bet + p.chips
    }

    pub fn can_raise(&self, seat: usize) -> bool {
        let p = &self.players[seat];
        // Есть фишки сверх колла и есть кому отвечать
        let others = self
            .players
            .iter()
            .enumerate()
            .any(|(i, o)| i != seat && o.can_act());
        p.bet + p.chips > self.current_bet && others
    }

    /// Применяет действие игрока, чей сейчас ход.
    pub fn act(&mut self, action: Action) {
        let Some(seat) = self.to_act else { return };
        let to_call = self.to_call(seat);
        let name = self.players[seat].name.clone();

        let action = match action {
            Action::Check if to_call > 0 => Action::Call,
            Action::Call if to_call == 0 => Action::Check,
            Action::RaiseTo(_) if !self.can_raise(seat) => {
                if to_call > 0 {
                    Action::Call
                } else {
                    Action::Check
                }
            }
            a => a,
        };

        let label = match action {
            Action::Fold => {
                self.players[seat].folded = true;
                "Фолд".to_string()
            }
            Action::Check => "Чек".to_string(),
            Action::Call => {
                let p = &mut self.players[seat];
                p.chips -= to_call;
                p.bet += to_call;
                p.total += to_call;
                if p.chips == 0 {
                    p.all_in = true;
                    "Олл-ин".to_string()
                } else {
                    format!("Колл {}", fmt_chips(to_call))
                }
            }
            Action::RaiseTo(t) => {
                let lo = self.min_raise_to(seat);
                let hi = self.max_raise_to(seat);
                let t = t.clamp(lo, hi);
                let raise_size = t - self.current_bet;
                if raise_size >= self.min_raise {
                    self.min_raise = raise_size;
                }
                let was_bet = self.current_bet == 0;
                self.current_bet = t;
                let p = &mut self.players[seat];
                let add = t - p.bet;
                p.chips -= add;
                p.bet = t;
                p.total += add;
                if p.chips == 0 {
                    p.all_in = true;
                }
                let all_in = p.all_in;
                for (i, o) in self.players.iter_mut().enumerate() {
                    if i != seat && o.can_act() {
                        o.acted = false;
                    }
                }
                if all_in {
                    "Олл-ин".to_string()
                } else if was_bet {
                    format!("Бет {}", fmt_chips(t))
                } else {
                    format!("Рейз {}", fmt_chips(t))
                }
            }
        };

        self.players[seat].acted = true;
        self.players[seat].last_action = Some(label.clone());
        self.push_log(format!("{}: {}", name, label));
        self.advance(seat);
    }

    fn round_complete(&self) -> bool {
        let able: Vec<&Player> = self.players.iter().filter(|p| p.can_act()).collect();
        if able.is_empty() {
            return true;
        }
        if able.len() == 1 {
            let p = able[0];
            // Остальные в олл-ине: единственному активному нужно лишь уравнять
            let others_contesting = self.players.iter().filter(|p| p.contesting()).count() > 1;
            if others_contesting && p.bet >= self.current_bet {
                return true;
            }
        }
        able.iter().all(|p| p.acted && p.bet == self.current_bet)
    }

    fn advance(&mut self, from: usize) {
        let contesting = self.players.iter().filter(|p| p.contesting()).count();
        if contesting == 1 {
            self.win_by_fold();
            return;
        }
        if self.round_complete() {
            self.next_street();
        } else {
            self.to_act = self.next_seat(from, |p| p.can_act());
        }
    }

    /// Возвращает неуравненную часть ставки.
    fn refund_uncalled(&mut self) {
        let mut bets: Vec<(u64, usize)> = self
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| (p.bet, i))
            .collect();
        bets.sort_by(|a, b| b.cmp(a));
        if bets.len() >= 2 && bets[0].0 > bets[1].0 {
            let diff = bets[0].0 - bets[1].0;
            let p = &mut self.players[bets[0].1];
            p.bet -= diff;
            p.total -= diff;
            p.chips += diff;
            if p.chips > 0 {
                p.all_in = false;
            }
        }
    }

    fn collect_bets(&mut self) {
        self.refund_uncalled();
        for p in &mut self.players {
            self.pot += p.bet;
            p.bet = 0;
            p.acted = false;
        }
        self.current_bet = 0;
        self.min_raise = self.bb;
    }

    fn next_street(&mut self) {
        self.seq += 1;
        self.collect_bets();
        let next = match self.street {
            Street::PreFlop => Some(Street::Flop),
            Street::Flop => Some(Street::Turn),
            Street::Turn => Some(Street::River),
            Street::River => None,
        };
        let Some(next) = next else {
            self.showdown();
            return;
        };
        self.street = next;
        let n = if next == Street::Flop { 3 } else { 1 };
        for _ in 0..n {
            let c = self.deck.draw();
            self.board.push(c);
        }
        for p in &mut self.players {
            if p.can_act() {
                p.last_action = None;
            }
        }
        if self.players.iter().filter(|p| p.can_act()).count() <= 1 {
            // Торговли больше нет — борд будет доложен через needs_runout()
            self.to_act = None;
        } else {
            self.to_act = self.next_seat(self.dealer, |p| p.can_act());
        }
    }

    /// Выкладывает следующую улицу без торговли (когда все в олл-ине).
    pub fn runout_step(&mut self) {
        if self.needs_runout() {
            self.next_street();
        }
    }

    fn win_by_fold(&mut self) {
        // Неуравненную ставку не возвращаем отдельно: победитель забирает всё
        for p in &mut self.players {
            self.pot += p.bet;
            p.bet = 0;
        }
        let seat = self.players.iter().position(|p| p.contesting()).unwrap();
        let amount = self.pot;
        self.players[seat].chips += amount;
        self.pot = 0;
        self.to_act = None;
        let text = format!("{} забирает банк {}", self.players[seat].name, fmt_chips(amount));
        self.push_log(text.clone());
        self.finish(HandResult {
            wins: vec![Win {
                seat,
                amount,
                hand: None,
            }],
            showdown: false,
            best_cards: Vec::new(),
            text,
        });
    }

    fn finish(&mut self, result: HandResult) {
        for p in &mut self.players {
            p.in_hand = p.in_hand && !p.folded;
        }
        self.result = Some(result);
    }

    fn showdown(&mut self) {
        self.to_act = None;
        let values: Vec<Option<HandValue>> = self
            .players
            .iter()
            .map(|p| {
                if p.contesting() {
                    let mut cards = p.hole.clone();
                    cards.extend_from_slice(&self.board);
                    Some(evaluate(&cards))
                } else {
                    None
                }
            })
            .collect();

        // Уровни вложений участников, дошедших до вскрытия → сайд-поты
        let mut levels: Vec<u64> = self
            .players
            .iter()
            .filter(|p| p.contesting())
            .map(|p| p.total)
            .collect();
        levels.sort_unstable();
        levels.dedup();

        let n = self.players.len();
        let mut won = vec![0u64; n];
        let mut prev = 0u64;
        let mut distributed = 0u64;
        for (li, &level) in levels.iter().enumerate() {
            let last = li == levels.len() - 1;
            let mut chunk: u64 = self
                .players
                .iter()
                .map(|p| p.total.min(level) - p.total.min(prev))
                .sum();
            if last {
                // Всё, что внесли сверх (например, сфолдившие), тоже сюда
                chunk = self.pot - distributed;
            }
            distributed += chunk;
            prev = level;
            let eligible: Vec<usize> = (0..n)
                .filter(|&i| self.players[i].contesting() && self.players[i].total >= level)
                .collect();
            let best = eligible.iter().filter_map(|&i| values[i]).max().unwrap();
            let mut winners: Vec<usize> = eligible
                .into_iter()
                .filter(|&i| values[i] == Some(best))
                .collect();
            // Нечётную фишку получает первый после дилера
            winners.sort_by_key(|&i| (i + n - self.dealer - 1) % n);
            let share = chunk / winners.len() as u64;
            let rem = chunk % winners.len() as u64;
            for (k, &w) in winners.iter().enumerate() {
                won[w] += share + if (k as u64) < rem { 1 } else { 0 };
            }
        }

        let mut wins = Vec::new();
        for i in 0..n {
            if won[i] > 0 {
                self.players[i].chips += won[i];
                wins.push(Win {
                    seat: i,
                    amount: won[i],
                    hand: values[i],
                });
            }
        }
        wins.sort_by(|a, b| b.amount.cmp(&a.amount));
        self.pot = 0;

        let main = &wins[0];
        let mut cards = self.players[main.seat].hole.clone();
        cards.extend_from_slice(&self.board);
        let best_cards = best_five(&cards);
        let text = wins
            .iter()
            .map(|w| {
                format!(
                    "{} выигрывает {} — {}",
                    self.players[w.seat].name,
                    fmt_chips(w.amount),
                    w.hand.map(|h| h.describe()).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        for line in text.lines() {
            self.push_log(line.to_string());
        }
        self.finish(HandResult {
            wins,
            showdown: true,
            best_cards,
            text,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> BotStyle {
        BotStyle {
            tight: 0.5,
            aggr: 0.5,
            bluff: 0.0,
        }
    }

    fn table(chips: &[u64]) -> Game {
        let players = chips
            .iter()
            .enumerate()
            .map(|(i, &c)| Player::new(&format!("P{}", i), c, false, style(), i))
            .collect();
        Game::new(players, 5, 10, Rng::new(1234))
    }

    fn total_chips(g: &Game) -> u64 {
        g.players.iter().map(|p| p.chips + p.bet).sum::<u64>() + g.pot
    }

    #[test]
    fn everyone_folds_to_big_blind() {
        let mut g = table(&[1000, 1000, 1000]);
        assert!(g.start_hand());
        while g.result.is_none() {
            g.act(Action::Fold);
        }
        assert_eq!(total_chips(&g), 3000);
        let w = &g.result.as_ref().unwrap().wins[0];
        assert_eq!(w.amount, 15);
    }

    #[test]
    fn chips_are_conserved_in_random_play() {
        let mut rng = Rng::new(99);
        for t in 0..300 {
            let mut g = table(&[1000, 500, 200, 3000, 50, 800]);
            g.rng = Rng::new(t + 1);
            for _ in 0..30 {
                if !g.start_hand() {
                    break;
                }
                let mut guard = 0;
                while g.result.is_none() {
                    guard += 1;
                    assert!(guard < 500, "раздача зациклилась");
                    if g.needs_runout() {
                        g.runout_step();
                        continue;
                    }
                    let seat = g.to_act.unwrap();
                    let a = match rng.below(5) {
                        0 => Action::Fold,
                        1 => Action::Check,
                        2 => Action::Call,
                        3 => Action::RaiseTo(g.min_raise_to(seat)),
                        _ => Action::RaiseTo(g.max_raise_to(seat)),
                    };
                    g.act(a);
                    assert_eq!(total_chips(&g), 5550);
                }
                assert_eq!(total_chips(&g), 5550);
                if g.result.as_ref().unwrap().showdown {
                    assert_eq!(g.board.len(), 5);
                }
            }
        }
    }

    #[test]
    fn side_pot_short_stack_wins_only_main() {
        use crate::cards::{Card, Suit::*};
        let mut g = table(&[100, 1000, 1000]);
        g.start_hand();
        // Подменяем карты: у короткого стека тузы, у остальных мусор
        let short = 0;
        g.players[short].hole = vec![Card::new(14, Spades), Card::new(14, Hearts)];
        g.players[1].hole = vec![Card::new(13, Spades), Card::new(13, Hearts)];
        g.players[2].hole = vec![Card::new(2, Clubs), Card::new(7, Diamonds)];
        // Все идут в олл-ин / коллируют
        while g.result.is_none() {
            if g.needs_runout() {
                g.runout_step();
                continue;
            }
            let s = g.to_act.unwrap();
            g.act(Action::RaiseTo(g.max_raise_to(s)));
        }
        assert_eq!(total_chips(&g), 2100);
        let r = g.result.as_ref().unwrap();
        assert!(r.showdown);
        // короткий стек не может выиграть больше 300
        assert!(g.players[short].chips <= 300);
    }
}
