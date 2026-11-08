//! Простой, но живой ИИ: оценивает эквити методом Монте-Карло
//! и принимает решения с учётом шансов банка и характера.

use crate::cards::{Card, Rng, full_deck};
use crate::eval::evaluate;
use crate::game::{Action, Game, Street};

/// Вероятность выигрыша (доля банка) против `opponents` случайных рук.
pub fn equity(hole: &[Card], board: &[Card], opponents: usize, iters: usize, rng: &mut Rng) -> f32 {
    if opponents == 0 {
        return 1.0;
    }
    let mut deck: Vec<Card> = full_deck()
        .into_iter()
        .filter(|c| !hole.contains(c) && !board.contains(c))
        .collect();
    let need_board = 5 - board.len();
    let mut score = 0.0f32;
    let mut my = Vec::with_capacity(7);
    let mut their = Vec::with_capacity(7);
    let mut full_board = Vec::with_capacity(5);
    for _ in 0..iters {
        // частичная перетасовка: нужно только need_board + 2*opponents карт
        let need = need_board + 2 * opponents;
        for i in 0..need {
            let j = i + rng.below(deck.len() - i);
            deck.swap(i, j);
        }
        full_board.clear();
        full_board.extend_from_slice(board);
        full_board.extend_from_slice(&deck[..need_board]);

        my.clear();
        my.extend_from_slice(hole);
        my.extend_from_slice(&full_board);
        let mine = evaluate(&my);

        let mut best_other = None;
        let mut ties = 0;
        let mut lost = false;
        for o in 0..opponents {
            let k = need_board + 2 * o;
            their.clear();
            their.extend_from_slice(&deck[k..k + 2]);
            their.extend_from_slice(&full_board);
            let v = evaluate(&their);
            if v > mine {
                lost = true;
                break;
            }
            if v == mine {
                ties += 1;
            }
            best_other = Some(v);
        }
        let _ = best_other;
        if !lost {
            score += 1.0 / (ties as f32 + 1.0);
        }
    }
    score / iters as f32
}

/// Решение бота, сидящего на месте `seat`.
pub fn decide(game: &mut Game, seat: usize) -> Action {
    let p = &game.players[seat];
    let style = p.style;
    let hole = p.hole.clone();
    let board = game.board.clone();
    let opponents = game
        .players
        .iter()
        .enumerate()
        .filter(|(i, o)| *i != seat && o.contesting())
        .count();
    let to_call = game.to_call(seat);
    let pot = game.total_pot();
    let chips = p.chips;
    let bb = game.bb;
    let can_raise = game.can_raise(seat);
    let min_to = game.min_raise_to(seat);
    let max_to = game.max_raise_to(seat);
    let current = game.current_bet;
    let my_bet = p.bet;

    let iters = if game.street == Street::PreFlop { 220 } else { 300 };
    let rng = &mut game.rng;
    let eq = equity(&hole, &board, opponents, iters, rng);

    // Сила относительно «средней» руки за столом (1.0 = средняя)
    let strength = eq * (opponents as f32 + 1.0);
    let pot_odds = if to_call == 0 {
        0.0
    } else {
        to_call as f32 / (pot + to_call) as f32
    };
    let r = rng.unit();
    let noise = (rng.unit() - 0.5) * 0.2;
    let s = strength + noise + style.aggr * 0.15 - style.tight * 0.15;

    // Размер ставки как доля банка
    let raise_to = |frac: f32| -> u64 {
        let base = pot.max(bb * 2) as f32 * frac;
        let target = current + (base as u64).max(bb);
        // округляем до малых блайндов, чтобы суммы выглядели «живыми»
        let unit = (bb / 2).max(1);
        let target = (target / unit) * unit;
        target.clamp(min_to, max_to)
    };

    let short = chips <= bb * 8;

    // Монстр
    if eq > 0.80 || s > 2.6 {
        if can_raise {
            if short || (eq > 0.88 && r < 0.25) {
                return Action::RaiseTo(max_to);
            }
            // Иногда слоуплей
            if to_call == 0 && r < 0.2 * (1.0 - style.aggr) {
                return Action::Check;
            }
            return Action::RaiseTo(raise_to(0.6 + style.aggr * 0.6));
        }
        return Action::Call;
    }

    // Сильная рука
    if s > 1.7 {
        if can_raise && r < 0.35 + style.aggr * 0.45 && current < my_bet + chips / 3 {
            return Action::RaiseTo(raise_to(0.45 + style.aggr * 0.4));
        }
        if to_call == 0 {
            return Action::Check;
        }
        if eq + 0.1 > pot_odds || to_call <= bb * 4 {
            return Action::Call;
        }
        return Action::Fold;
    }

    // Средняя рука / дро
    let margin = 0.02 + style.tight * 0.08;
    if to_call == 0 {
        if can_raise && r < style.bluff + style.aggr * 0.1 && opponents <= 3 {
            return Action::RaiseTo(raise_to(0.5));
        }
        return Action::Check;
    }
    if eq > pot_odds + margin {
        // не отдаём весь стек со средней рукой
        if to_call > chips / 2 && s < 1.3 && r > 0.25 {
            return Action::Fold;
        }
        return Action::Call;
    }
    // Блеф-рейз
    if can_raise && r < style.bluff * 0.6 && to_call <= bb * 3 && opponents <= 2 {
        return Action::RaiseTo(raise_to(0.7));
    }
    // Дешёвый колл из лимпов
    if to_call <= bb && game.street == Street::PreFlop && r > style.tight {
        return Action::Call;
    }
    Action::Fold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Suit::*;

    #[test]
    fn aces_are_favourites() {
        let mut rng = Rng::new(5);
        let aa = [Card::new(14, Spades), Card::new(14, Hearts)];
        let e = equity(&aa, &[], 1, 2000, &mut rng);
        assert!(e > 0.78 && e < 0.9, "AA vs random = {}", e);
        let trash = [Card::new(2, Spades), Card::new(7, Hearts)];
        let e2 = equity(&trash, &[], 1, 2000, &mut rng);
        assert!(e2 < 0.4, "72o = {}", e2);
    }
}
