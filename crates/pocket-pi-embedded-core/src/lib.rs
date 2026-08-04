#![cfg_attr(not(test), no_std)]

/// Connectivity is tracked independently for every external dependency so the
/// display can distinguish a Wi-Fi failure from an authentication failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkState {
    Disabled,
    Connecting,
    Online,
    Degraded,
    Offline,
}

/// High-level agent lifecycle exposed to the display layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentState {
    Stopped,
    Starting,
    Idle,
    Thinking,
    Acting,
    Faulted,
}

/// Authentication modes are explicit because Coding Plan and API billing are
/// materially different operational modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexAuthMode {
    Unconfigured,
    CodingPlan,
    ApiKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TradingMode {
    ReadOnly,
    ConfirmEachOrder,
    Automated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderDecision {
    Allowed,
    ReadOnly,
    ConfirmationRequired,
    OrderLimitExceeded,
    DailyLimitExceeded,
}

/// Device-enforced limits are checked after the model proposes an order and
/// before any broker tool is called. Broker/account controls remain a second,
/// independent layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TradingPolicy {
    pub mode: TradingMode,
    pub max_order_notional_micros: u64,
    pub max_daily_notional_micros: u64,
}

impl Default for TradingPolicy {
    fn default() -> Self {
        Self {
            mode: TradingMode::ReadOnly,
            max_order_notional_micros: 0,
            max_daily_notional_micros: 0,
        }
    }
}

impl TradingPolicy {
    pub fn evaluate_order(
        &self,
        notional_micros: u64,
        daily_notional_micros: u64,
        confirmed: bool,
    ) -> OrderDecision {
        if self.mode == TradingMode::ReadOnly {
            return OrderDecision::ReadOnly;
        }
        if self.mode == TradingMode::ConfirmEachOrder && !confirmed {
            return OrderDecision::ConfirmationRequired;
        }
        if notional_micros > self.max_order_notional_micros {
            return OrderDecision::OrderLimitExceeded;
        }
        if daily_notional_micros.saturating_add(notional_micros) > self.max_daily_notional_micros {
            return OrderDecision::DailyLimitExceeded;
        }
        OrderDecision::Allowed
    }
}

/// A compact broker projection. The device keeps no order-entry credentials in
/// this structure and can continue showing the last known values while offline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortfolioSummary {
    pub equity_micros: i64,
    pub buying_power_micros: i64,
    pub day_change_micros: i64,
    pub open_positions: u16,
    pub version: u64,
    pub updated_at_unix_ms: u64,
}

impl PortfolioSummary {
    pub const EMPTY: Self = Self {
        equity_micros: 0,
        buying_power_micros: 0,
        day_change_micros: 0,
        open_positions: 0,
        version: 0,
        updated_at_unix_ms: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceState {
    pub wifi: LinkState,
    pub codex: LinkState,
    pub robinhood: LinkState,
    pub agent: AgentState,
    pub codex_auth: CodexAuthMode,
    pub portfolio: PortfolioSummary,
    pub last_error_code: Option<u16>,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            wifi: LinkState::Offline,
            codex: LinkState::Disabled,
            robinhood: LinkState::Disabled,
            agent: AgentState::Stopped,
            codex_auth: CodexAuthMode::Unconfigured,
            portfolio: PortfolioSummary::EMPTY,
            last_error_code: None,
        }
    }
}

impl DeviceState {
    pub fn begin_boot(&mut self) {
        self.wifi = LinkState::Connecting;
        self.agent = AgentState::Starting;
        self.last_error_code = None;
    }

    pub fn set_network_online(&mut self) {
        self.wifi = LinkState::Online;
        self.codex = LinkState::Connecting;
        self.robinhood = LinkState::Connecting;
    }

    pub fn apply_portfolio(&mut self, next: PortfolioSummary) -> bool {
        if next.version <= self.portfolio.version {
            return false;
        }

        self.portfolio = next;
        self.robinhood = LinkState::Online;
        true
    }

    /// A disconnect changes freshness indicators but deliberately preserves the
    /// last portfolio projection so the screen never flashes misleading zeros.
    pub fn set_network_offline(&mut self, error_code: u16) {
        self.wifi = LinkState::Offline;
        self.codex = LinkState::Offline;
        self.robinhood = LinkState::Offline;
        self.agent = AgentState::Faulted;
        self.last_error_code = Some(error_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portfolio(version: u64, equity_micros: i64) -> PortfolioSummary {
        PortfolioSummary {
            equity_micros,
            buying_power_micros: 2_000_000,
            day_change_micros: 50_000,
            open_positions: 3,
            version,
            updated_at_unix_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn stale_broker_updates_cannot_roll_back_the_display() {
        let mut state = DeviceState::default();

        assert!(state.apply_portfolio(portfolio(2, 10_000_000)));
        assert!(!state.apply_portfolio(portfolio(1, 1_000_000)));
        assert_eq!(state.portfolio.equity_micros, 10_000_000);
    }

    #[test]
    fn going_offline_preserves_last_known_portfolio() {
        let mut state = DeviceState::default();
        state.apply_portfolio(portfolio(1, 10_000_000));

        state.set_network_offline(12);

        assert_eq!(state.wifi, LinkState::Offline);
        assert_eq!(state.portfolio.equity_micros, 10_000_000);
        assert_eq!(state.last_error_code, Some(12));
    }

    #[test]
    fn coding_plan_and_api_key_are_distinct_modes() {
        let state = DeviceState {
            codex_auth: CodexAuthMode::CodingPlan,
            ..DeviceState::default()
        };
        assert_ne!(state.codex_auth, CodexAuthMode::ApiKey);
    }

    #[test]
    fn trading_is_read_only_by_default() {
        assert_eq!(
            TradingPolicy::default().evaluate_order(1, 0, true),
            OrderDecision::ReadOnly
        );
    }

    #[test]
    fn device_limits_apply_after_confirmation() {
        let policy = TradingPolicy {
            mode: TradingMode::ConfirmEachOrder,
            max_order_notional_micros: 10_000_000,
            max_daily_notional_micros: 20_000_000,
        };

        assert_eq!(
            policy.evaluate_order(5_000_000, 0, false),
            OrderDecision::ConfirmationRequired
        );
        assert_eq!(
            policy.evaluate_order(11_000_000, 0, true),
            OrderDecision::OrderLimitExceeded
        );
        assert_eq!(
            policy.evaluate_order(5_000_000, 18_000_000, true),
            OrderDecision::DailyLimitExceeded
        );
        assert_eq!(
            policy.evaluate_order(5_000_000, 10_000_000, true),
            OrderDecision::Allowed
        );
    }
}
