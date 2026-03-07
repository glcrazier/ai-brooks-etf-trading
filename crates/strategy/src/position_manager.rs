use std::collections::HashMap;

use brooks_core::market::{Direction, SecurityId};
use brooks_core::position::Position;
use brooks_pa_engine::trend::SwingPoint;
use rust_decimal::Decimal;

use crate::stop_target::StopTargetCalculator;
use crate::traits::StrategyAction;

/// A position with additional management metadata.
#[derive(Debug, Clone)]
pub struct ManagedPosition {
    pub position: Position,
    pub original_stop: Decimal,
    pub original_target: Option<Decimal>,
    /// Risk per unit at entry (entry_price - stop_price)
    pub risk_per_unit: Decimal,
    /// Whether trailing stop is activated (after 1R profit)
    pub trail_active: bool,
    /// Number of partial exits taken
    pub partial_exits: u32,
    /// Number of bars held
    pub bars_held: u64,
}

/// Tracks open positions and manages stop/target exit decisions.
pub struct PositionManager {
    positions: HashMap<SecurityId, ManagedPosition>,
    stop_target_calc: StopTargetCalculator,
}

impl PositionManager {
    pub fn new(stop_target_calc: StopTargetCalculator) -> Self {
        Self {
            positions: HashMap::new(),
            stop_target_calc,
        }
    }

    /// Add a new position after a fill.
    pub fn add_position(
        &mut self,
        position: Position,
        risk_per_unit: Decimal,
        target: Option<Decimal>,
    ) {
        let security = position.security.clone();
        let managed = ManagedPosition {
            original_stop: position.stop_loss,
            original_target: target,
            risk_per_unit,
            trail_active: false,
            partial_exits: 0,
            bars_held: 0,
            position,
        };
        self.positions.insert(security, managed);
    }

    /// Remove a position after close.
    pub fn remove_position(&mut self, security: &SecurityId) -> Option<ManagedPosition> {
        self.positions.remove(security)
    }

    /// Update a position with new market data and return any actions needed.
    ///
    /// Checks: stop hit, target hit, trailing stop update, increments bars_held.
    pub fn update(
        &mut self,
        security: &SecurityId,
        current_price: Decimal,
        swing_lows: &[SwingPoint],
        swing_highs: &[SwingPoint],
    ) -> Vec<StrategyAction> {
        let mut actions = Vec::new();

        let Some(managed) = self.positions.get_mut(security) else {
            return actions;
        };

        // Update current price
        managed.position.update_price(current_price);
        managed.bars_held += 1;

        // 1. Check stop loss hit
        if managed.position.is_stop_hit(current_price) {
            actions.push(StrategyAction::ClosePosition {
                security: security.clone(),
                reason: "Stop loss hit".to_string(),
            });
            return actions;
        }

        // 2. Check target hit
        if managed.position.is_target_hit(current_price) {
            actions.push(StrategyAction::ClosePosition {
                security: security.clone(),
                reason: "Target hit".to_string(),
            });
            return actions;
        }

        // 3. Check if trailing stop should activate (after 1R profit)
        if !managed.trail_active && managed.risk_per_unit > Decimal::ZERO {
            let unrealized_per_unit = match managed.position.direction {
                Direction::Long => current_price - managed.position.entry_price,
                Direction::Short => managed.position.entry_price - current_price,
            };
            if unrealized_per_unit >= managed.risk_per_unit {
                managed.trail_active = true;
            }
        }

        // 4. Update trailing stop if active
        if managed.trail_active {
            let new_stop = self.stop_target_calc.trailing_stop(
                managed.position.direction,
                managed.position.stop_loss,
                managed.position.entry_price,
                swing_lows,
                swing_highs,
            );

            if let Some(new_stop) = new_stop {
                managed.position.stop_loss = new_stop;
                actions.push(StrategyAction::UpdateStopLoss {
                    security: security.clone(),
                    new_stop,
                });
            }
        }

        actions
    }

    /// Force close all positions (end of day or session close).
    pub fn close_all(&self, reason: &str) -> Vec<StrategyAction> {
        self.positions
            .keys()
            .map(|security| StrategyAction::ClosePosition {
                security: security.clone(),
                reason: reason.to_string(),
            })
            .collect()
    }

    /// Get all open positions.
    pub fn positions(&self) -> Vec<&Position> {
        self.positions.values().map(|mp| &mp.position).collect()
    }

    /// Number of open positions.
    pub fn count(&self) -> usize {
        self.positions.len()
    }

    /// Whether a position exists for the given security.
    pub fn has_position(&self, security: &SecurityId) -> bool {
        self.positions.contains_key(security)
    }

    /// Get a managed position by security.
    pub fn get_position(&self, security: &SecurityId) -> Option<&ManagedPosition> {
        self.positions.get(security)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brooks_core::market::Exchange;
    use brooks_pa_engine::trend::SwingPointType;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn etf_510050() -> SecurityId {
        SecurityId::etf("510050", Exchange::SH)
    }

    fn long_position(entry: Decimal, stop: Decimal, target: Option<Decimal>) -> Position {
        Position {
            security: etf_510050(),
            direction: Direction::Long,
            quantity: 1000,
            entry_price: entry,
            current_price: entry,
            stop_loss: stop,
            take_profit: target,
            opened_at: Utc::now(),
        }
    }

    fn make_manager() -> PositionManager {
        PositionManager::new(StopTargetCalculator::new(dec!(1.5)))
    }

    #[test]
    fn test_add_and_has_position() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), Some(dec!(3.600)));
        mgr.add_position(pos, dec!(0.050), Some(dec!(3.600)));
        assert!(mgr.has_position(&etf_510050()));
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn test_remove_position() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), Some(dec!(3.600)));
        mgr.add_position(pos, dec!(0.050), Some(dec!(3.600)));
        let removed = mgr.remove_position(&etf_510050());
        assert!(removed.is_some());
        assert!(!mgr.has_position(&etf_510050()));
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_stop_hit_triggers_close() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), Some(dec!(3.600)));
        mgr.add_position(pos, dec!(0.050), Some(dec!(3.600)));

        // Price drops to stop
        let actions = mgr.update(&etf_510050(), dec!(3.440), &[], &[]);
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], StrategyAction::ClosePosition { reason, .. } if reason.contains("Stop")));
    }

    #[test]
    fn test_target_hit_triggers_close() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), Some(dec!(3.600)));
        mgr.add_position(pos, dec!(0.050), Some(dec!(3.600)));

        // Price reaches target
        let actions = mgr.update(&etf_510050(), dec!(3.610), &[], &[]);
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], StrategyAction::ClosePosition { reason, .. } if reason.contains("Target")));
    }

    #[test]
    fn test_trailing_stop_activates_after_1r() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), None);
        mgr.add_position(pos, dec!(0.050), None);

        // Price moves up by 1R (0.050) -> 3.550
        let actions = mgr.update(&etf_510050(), dec!(3.550), &[], &[]);
        // No trailing stop action yet (no swing points above stop)
        assert!(actions.is_empty());
        // But trail_active should be true
        let managed = mgr.get_position(&etf_510050()).unwrap();
        assert!(managed.trail_active);
    }

    #[test]
    fn test_trailing_stop_updates_with_swing() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), None);
        mgr.add_position(pos, dec!(0.050), None);

        // First, activate trailing (1R profit)
        mgr.update(&etf_510050(), dec!(3.560), &[], &[]);

        // Now provide a swing low above stop
        let swing_lows = vec![SwingPoint {
            price: dec!(3.480),
            bar_index: 10,
            timestamp: Utc::now(),
            point_type: SwingPointType::Low,
        }];
        let actions = mgr.update(&etf_510050(), dec!(3.570), &swing_lows, &[]);

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            StrategyAction::UpdateStopLoss { new_stop, .. } if *new_stop == dec!(3.480)
        ));
    }

    #[test]
    fn test_bars_held_increments() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), None);
        mgr.add_position(pos, dec!(0.050), None);

        mgr.update(&etf_510050(), dec!(3.510), &[], &[]);
        mgr.update(&etf_510050(), dec!(3.520), &[], &[]);
        mgr.update(&etf_510050(), dec!(3.530), &[], &[]);

        let managed = mgr.get_position(&etf_510050()).unwrap();
        assert_eq!(managed.bars_held, 3);
    }

    #[test]
    fn test_close_all() {
        let mut mgr = make_manager();

        let pos1 = long_position(dec!(3.500), dec!(3.450), None);
        mgr.add_position(pos1, dec!(0.050), None);

        let mut pos2 = long_position(dec!(4.000), dec!(3.950), None);
        pos2.security = SecurityId::etf("510300", Exchange::SH);
        mgr.add_position(pos2, dec!(0.050), None);

        let actions = mgr.close_all("End of day");
        assert_eq!(actions.len(), 2);
        for action in &actions {
            assert!(matches!(action, StrategyAction::ClosePosition { reason, .. } if reason == "End of day"));
        }
    }

    #[test]
    fn test_positions_getter() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), None);
        mgr.add_position(pos, dec!(0.050), None);
        let positions = mgr.positions();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].entry_price, dec!(3.500));
    }

    #[test]
    fn test_no_trailing_stop_before_1r() {
        let mut mgr = make_manager();
        let pos = long_position(dec!(3.500), dec!(3.450), None);
        mgr.add_position(pos, dec!(0.050), None);

        // Price moves up less than 1R
        let swing_lows = vec![SwingPoint {
            price: dec!(3.470),
            bar_index: 10,
            timestamp: Utc::now(),
            point_type: SwingPointType::Low,
        }];
        let actions = mgr.update(&etf_510050(), dec!(3.530), &swing_lows, &[]);

        // Should not activate trailing stop yet
        assert!(actions.is_empty());
        let managed = mgr.get_position(&etf_510050()).unwrap();
        assert!(!managed.trail_active);
    }
}
