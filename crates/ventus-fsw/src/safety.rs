//! Safety / mode / critical-discrete kernel — ADR-005.
//!
//! Sits *beside* [`crate::step`], not inside it. The pitch PI still owns
//! air-data hygiene; this module owns whether that PI is allowed to write a
//! new surface command.
//!
//! # What this is
//!
//! A total enum state machine, four guarded discretes, and a software
//! frame-counter watchdog. Every hot-path value is `Copy`, bounded, and
//! allocation-free. Illegal events stay. Abort cannot be dropped on the
//! floor without a test failing.
//!
//! # What this is not
//!
//! Flight certification, a GUI, ejector-seat physics, voting hardware, or
//! GPIO sampling. [`DiscreteFrame::to_wire`] is the HIL discrete byte
//! (ADR-004 D6).

use crate::Health;

/// Consecutive resolved asserts required before a latched discrete takes, or
/// a momentary discrete fires. `[TO DETERMINE]` against HIL sample timing.
pub const DISCRETE_CONFIRM_FRAMES: u8 = 2;

/// Good BIT frames required before `Bit` → `Ready`. Software walk only.
pub const BIT_PASS_FRAMES: u8 = 5;

/// Frames spent in `Bit` without passing → `Safe`. `[TO DETERMINE]`.
pub const BIT_TIMEOUT_FRAMES: u8 = 50;

/// Consecutive missing heartbeats → [`FlightMode::Degraded`]. `[TO DETERMINE]`.
/// At [`crate::CONTROL_RATE_HZ`] this is 30 ms; that rate is itself declared,
/// not measured on a flight computer.
pub const WATCHDOG_DEGRADED_MISSES: u8 = 3;

/// Consecutive missing heartbeats → [`FlightMode::Safe`]. `[TO DETERMINE]`.
pub const WATCHDOG_SAFE_MISSES: u8 = 10;

/// Ticks of [`FlightMode::Emergency`] before the next tick enters `Safe`.
/// Detection is one observable frame; `Safe` is the latched end-state.
pub const EMERGENCY_TO_SAFE_FRAMES: u8 = 1;

const _: () = assert!(WATCHDOG_DEGRADED_MISSES < WATCHDOG_SAFE_MISSES);
const _: () = assert!(WATCHDOG_DEGRADED_MISSES > 0);
const _: () = assert!(BIT_PASS_FRAMES > 0);
const _: () = assert!(BIT_TIMEOUT_FRAMES > BIT_PASS_FRAMES);
const _: () = assert!(DISCRETE_CONFIRM_FRAMES >= 2);
const _: () = assert!(EMERGENCY_TO_SAFE_FRAMES >= 1);

/// Vehicle-level mode. Distinct from [`Health`], which is this frame's air data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightMode {
    /// Power-on. No BIT yet.
    Startup,
    /// Software power-on test. Not a hardware BIT, not coverage of the airframe.
    Bit,
    /// BIT passed. Effectors not armed.
    Ready,
    /// Armed. The pitch loop may command surfaces.
    Nominal,
    /// Watchdog miss band or loop health not Nominal, without an abort.
    Degraded,
    /// Abort or emergency discrete confirmed. Detection state, not a dwell.
    Emergency,
    /// Latched end-state. ResetBIT is the only way out, and not to Nominal.
    Safe,
}

impl FlightMode {
    /// Wire tag. This is also the HIL SAFETY_TICK / GATED_STEP reply byte
    /// for mode (ADR-004 D6). A value that is not one of these tags is
    /// [`None`], never a panic and never a silent `Startup`.
    #[must_use]
    pub const fn to_wire(self) -> u8 {
        match self {
            FlightMode::Startup => 0,
            FlightMode::Bit => 1,
            FlightMode::Ready => 2,
            FlightMode::Nominal => 3,
            FlightMode::Degraded => 4,
            FlightMode::Emergency => 5,
            FlightMode::Safe => 6,
        }
    }

    /// Inverse of [`Self::to_wire`]. Totality over `u8`: unknown → `None`.
    #[must_use]
    pub const fn from_wire(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(FlightMode::Startup),
            1 => Some(FlightMode::Bit),
            2 => Some(FlightMode::Ready),
            3 => Some(FlightMode::Nominal),
            4 => Some(FlightMode::Degraded),
            5 => Some(FlightMode::Emergency),
            6 => Some(FlightMode::Safe),
            _ => None,
        }
    }

    pub const ALL: [FlightMode; 7] = [
        FlightMode::Startup,
        FlightMode::Bit,
        FlightMode::Ready,
        FlightMode::Nominal,
        FlightMode::Degraded,
        FlightMode::Emergency,
        FlightMode::Safe,
    ];

    /// Total transition function. Illegal events leave `self` unchanged.
    /// Abort / declare-emergency are accepted from every mode; `Safe` stays
    /// `Safe` because it is already the fail-safe.
    #[must_use]
    pub const fn apply(self, event: ModeEvent) -> Self {
        match event {
            ModeEvent::Abort | ModeEvent::DeclareEmergency => match self {
                FlightMode::Safe => FlightMode::Safe,
                FlightMode::Emergency => FlightMode::Emergency,
                FlightMode::Startup
                | FlightMode::Bit
                | FlightMode::Ready
                | FlightMode::Nominal
                | FlightMode::Degraded => FlightMode::Emergency,
            },
            ModeEvent::EnterSafe => FlightMode::Safe,
            ModeEvent::BeginBit => match self {
                FlightMode::Startup => FlightMode::Bit,
                other => other,
            },
            ModeEvent::BitPass => match self {
                FlightMode::Bit => FlightMode::Ready,
                other => other,
            },
            ModeEvent::BitFail => match self {
                FlightMode::Bit => FlightMode::Safe,
                other => other,
            },
            ModeEvent::Arm => match self {
                FlightMode::Ready => FlightMode::Nominal,
                other => other,
            },
            ModeEvent::Disarm => match self {
                FlightMode::Nominal | FlightMode::Degraded => FlightMode::Ready,
                other => other,
            },
            ModeEvent::Degrade => match self {
                FlightMode::Ready | FlightMode::Nominal | FlightMode::Degraded => {
                    FlightMode::Degraded
                }
                other => other,
            },
            ModeEvent::Recover => match self {
                FlightMode::Degraded => FlightMode::Nominal,
                other => other,
            },
            ModeEvent::ResetBit => match self {
                FlightMode::Safe | FlightMode::Bit => FlightMode::Bit,
                other => other,
            },
        }
    }
}

/// Events the mode table accepts. The kernel decides *which* event a frame
/// produced; this type does not carry "garbage" — unknown wire bytes never
/// become an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeEvent {
    BeginBit,
    BitPass,
    BitFail,
    Arm,
    Disarm,
    Degrade,
    Recover,
    Abort,
    DeclareEmergency,
    EnterSafe,
    ResetBit,
}

impl ModeEvent {
    pub const ALL: [ModeEvent; 11] = [
        ModeEvent::BeginBit,
        ModeEvent::BitPass,
        ModeEvent::BitFail,
        ModeEvent::Arm,
        ModeEvent::Disarm,
        ModeEvent::Degrade,
        ModeEvent::Recover,
        ModeEvent::Abort,
        ModeEvent::DeclareEmergency,
        ModeEvent::EnterSafe,
        ModeEvent::ResetBit,
    ];
}

/// Whether the pitch loop may write a new surface command this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAuthority {
    /// [`crate::step`] may compute and apply a command. [`FlightMode::Nominal`] only.
    Command,
    /// Last accepted command and integrator are frozen.
    HoldLast,
}

impl SurfaceAuthority {
    #[must_use]
    pub const fn for_mode(mode: FlightMode) -> Self {
        match mode {
            FlightMode::Nominal => SurfaceAuthority::Command,
            FlightMode::Startup
            | FlightMode::Bit
            | FlightMode::Ready
            | FlightMode::Degraded
            | FlightMode::Emergency
            | FlightMode::Safe => SurfaceAuthority::HoldLast,
        }
    }

    #[must_use]
    pub const fn allows_surface_command(self) -> bool {
        match self {
            SurfaceAuthority::Command => true,
            SurfaceAuthority::HoldLast => false,
        }
    }

    /// Wire tag: 0 HoldLast, 1 Command. HIL reply byte 1 (ADR-004 D6).
    #[must_use]
    pub const fn to_wire(self) -> u8 {
        match self {
            SurfaceAuthority::HoldLast => 0,
            SurfaceAuthority::Command => 1,
        }
    }

    /// Inverse of [`Self::to_wire`]. Unknown -> `None`.
    #[must_use]
    pub const fn from_wire(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(SurfaceAuthority::HoldLast),
            1 => Some(SurfaceAuthority::Command),
            _ => None,
        }
    }
}

/// Snapshot produced by [`SafetyKernel::tick`]. `Copy`, fixed size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlGate {
    pub mode: FlightMode,
    pub authority: SurfaceAuthority,
    pub abort_latched: bool,
    pub emergency_latched: bool,
    pub master_arm_latched: bool,
    pub watchdog: WatchdogLevel,
}

impl ControlGate {
    #[must_use]
    pub const fn allows_surface_command(self) -> bool {
        self.authority.allows_surface_command()
    }
}

/// The four critical discretes this cut owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticalDiscrete {
    /// Flight abort. Software-latched until ResetBIT.
    Abort,
    /// Declare emergency. Same mode effect as abort in this cut; distinct latch.
    Emergency,
    /// Effector arm — allows the pitch loop to command surfaces. **Not** a
    /// weapons bus. This repository is not weapons work.
    MasterArm,
    /// Momentary BIT reset. Single-channel `[TO DETERMINE]`.
    ResetBit,
}

/// Latch vs edge, stated in the type so a comment cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscreteKind {
    /// Stays asserted in software once confirmed, until a documented clear.
    Latched,
    /// Confirmed pulse, consumed once. Holding the line does not re-fire.
    Momentary,
}

/// How two samples of one discrete are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelPolicy {
    /// Channel A only. Channel B is carried and ignored.
    ///
    /// `[TO DETERMINE]` — declaring dual here would be a redundancy claim
    /// this cut has not earned.
    Single,
    /// Both channels must be read. Disagreement is [`DisagreeAction`], not
    /// "pick A".
    DualAgree,
}

/// What a dual-channel split does. Arm fails closed; abort fails toward Safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisagreeAction {
    /// Stay at the fail-safe deasserted level. Used for arming.
    Deassert,
    /// Treat as asserted. Used for abort/emergency so a split cannot be flown.
    Assert,
}

impl CriticalDiscrete {
    #[must_use]
    pub const fn kind(self) -> DiscreteKind {
        match self {
            CriticalDiscrete::Abort | CriticalDiscrete::Emergency | CriticalDiscrete::MasterArm => {
                DiscreteKind::Latched
            }
            CriticalDiscrete::ResetBit => DiscreteKind::Momentary,
        }
    }

    #[must_use]
    pub const fn channel_policy(self) -> ChannelPolicy {
        match self {
            CriticalDiscrete::Abort | CriticalDiscrete::Emergency | CriticalDiscrete::MasterArm => {
                ChannelPolicy::DualAgree
            }
            CriticalDiscrete::ResetBit => ChannelPolicy::Single,
        }
    }

    #[must_use]
    pub const fn confirm_frames(self) -> u8 {
        match self {
            CriticalDiscrete::Abort
            | CriticalDiscrete::Emergency
            | CriticalDiscrete::MasterArm
            | CriticalDiscrete::ResetBit => DISCRETE_CONFIRM_FRAMES,
        }
    }

    #[must_use]
    pub const fn on_disagree(self) -> DisagreeAction {
        match self {
            CriticalDiscrete::Abort | CriticalDiscrete::Emergency => DisagreeAction::Assert,
            CriticalDiscrete::MasterArm => DisagreeAction::Deassert,
            // Single-channel: never consulted. Listed so a new variant fails to compile.
            CriticalDiscrete::ResetBit => DisagreeAction::Deassert,
        }
    }

    /// Fail-safe is deasserted for every discrete in this cut.
    #[must_use]
    pub const fn fail_safe_asserted(self) -> bool {
        match self {
            CriticalDiscrete::Abort
            | CriticalDiscrete::Emergency
            | CriticalDiscrete::MasterArm
            | CriticalDiscrete::ResetBit => false,
        }
    }

    /// Software latch clears when the line has been deasserted for the confirm
    /// window. Abort/emergency do **not**: they wait for ResetBIT.
    #[must_use]
    pub const fn drops_on_deassert(self) -> bool {
        match self {
            CriticalDiscrete::MasterArm => true,
            CriticalDiscrete::Abort | CriticalDiscrete::Emergency | CriticalDiscrete::ResetBit => {
                false
            }
        }
    }
}

/// Two independent samples of one discrete. Fail-safe is both deasserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DualSample {
    pub a: bool,
    pub b: bool,
}

impl DualSample {
    pub const DEASSERTED: Self = Self { a: false, b: false };

    #[must_use]
    pub const fn both(asserted: bool) -> Self {
        Self {
            a: asserted,
            b: asserted,
        }
    }

    #[must_use]
    pub const fn split(a: bool, b: bool) -> Self {
        Self { a, b }
    }
}

/// One control-frame sample of every critical discrete.
///
/// Fail-safe default: all deasserted. [`Self::to_wire`] / [`Self::from_wire`]
/// is the HIL discrete byte (ADR-004 D6): byte 0 of SAFETY_TICK, byte 32 of
/// GATED_STEP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiscreteFrame {
    pub abort: DualSample,
    pub emergency: DualSample,
    pub master_arm: DualSample,
    pub reset_bit: DualSample,
}

impl DiscreteFrame {
    #[must_use]
    pub const fn fail_safe() -> Self {
        Self {
            abort: DualSample::DEASSERTED,
            emergency: DualSample::DEASSERTED,
            master_arm: DualSample::DEASSERTED,
            reset_bit: DualSample::DEASSERTED,
        }
    }

    /// Pack eight bits: abort A/B, emergency A/B, master-arm A/B, reset-bit A/B.
    #[must_use]
    pub const fn to_wire(self) -> u8 {
        (self.abort.a as u8)
            | (self.abort.b as u8) << 1
            | (self.emergency.a as u8) << 2
            | (self.emergency.b as u8) << 3
            | (self.master_arm.a as u8) << 4
            | (self.master_arm.b as u8) << 5
            | (self.reset_bit.a as u8) << 6
            | (self.reset_bit.b as u8) << 7
    }

    /// Inverse of [`Self::to_wire`]. High bits of a `u16` are ignored so a
    /// garbage word cannot panic.
    #[must_use]
    pub const fn from_wire(bits: u16) -> Self {
        Self {
            abort: DualSample::split(bit(bits, 0), bit(bits, 1)),
            emergency: DualSample::split(bit(bits, 2), bit(bits, 3)),
            master_arm: DualSample::split(bit(bits, 4), bit(bits, 5)),
            reset_bit: DualSample::split(bit(bits, 6), bit(bits, 7)),
        }
    }
}

const fn bit(bits: u16, n: u8) -> bool {
    (bits & (1 << n)) != 0
}

/// Resolve a sample against the discrete's channel policy. Bounded, no loop.
#[must_use]
pub const fn resolve_sample(discrete: CriticalDiscrete, sample: DualSample) -> bool {
    match discrete.channel_policy() {
        ChannelPolicy::Single => sample.a,
        ChannelPolicy::DualAgree => {
            if sample.a == sample.b {
                sample.a
            } else {
                match discrete.on_disagree() {
                    DisagreeAction::Deassert => false,
                    DisagreeAction::Assert => true,
                }
            }
        }
    }
}

/// Software heartbeat. Presence this control frame, nothing else.
///
/// Not an STM32 IWDG, not a windowed peripheral, not a claim that hardware
/// is being kicked. HIL SAFETY_TICK sets [`Heartbeat::present`] from flag
/// bit 0 of the request (ADR-004 D6). GPIO / IWDG are a later bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heartbeat {
    pub present: bool,
}

impl Heartbeat {
    pub const PRESENT: Self = Self { present: true };
    pub const MISSING: Self = Self { present: false };
}

/// Level a [`Watchdog`] reports after a tick (or from the current miss count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogLevel {
    Ok,
    Degraded,
    TripSafe,
}

impl WatchdogLevel {
    /// Wire tag: 0 Ok, 1 Degraded, 2 TripSafe. HIL reply byte 3 (ADR-004 D6).
    #[must_use]
    pub const fn to_wire(self) -> u8 {
        match self {
            WatchdogLevel::Ok => 0,
            WatchdogLevel::Degraded => 1,
            WatchdogLevel::TripSafe => 2,
        }
    }

    /// Inverse of [`Self::to_wire`]. Unknown -> `None`.
    #[must_use]
    pub const fn from_wire(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(WatchdogLevel::Ok),
            1 => Some(WatchdogLevel::Degraded),
            2 => Some(WatchdogLevel::TripSafe),
            _ => None,
        }
    }
}

/// Consecutive missing control-frame heartbeats.
///
/// Typed and testable without a board. Clearing the counter does not itself
/// leave [`FlightMode::Safe`]; that latch is the kernel's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watchdog {
    consecutive_misses: u8,
}

impl Watchdog {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            consecutive_misses: 0,
        }
    }

    #[must_use]
    pub const fn consecutive_misses(self) -> u8 {
        self.consecutive_misses
    }

    #[must_use]
    pub const fn level(self) -> WatchdogLevel {
        if self.consecutive_misses >= WATCHDOG_SAFE_MISSES {
            WatchdogLevel::TripSafe
        } else if self.consecutive_misses >= WATCHDOG_DEGRADED_MISSES {
            WatchdogLevel::Degraded
        } else {
            WatchdogLevel::Ok
        }
    }

    pub fn reset(&mut self) {
        self.consecutive_misses = 0;
    }

    pub fn tick(&mut self, heartbeat: Heartbeat) -> WatchdogLevel {
        if heartbeat.present {
            self.consecutive_misses = 0;
        } else {
            self.consecutive_misses = self.consecutive_misses.saturating_add(1);
        }
        self.level()
    }
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiscreteFilter {
    consecutive_asserted: u8,
    consecutive_deasserted: u8,
    latched: bool,
    resolved: bool,
}

impl DiscreteFilter {
    const fn new() -> Self {
        Self {
            consecutive_asserted: 0,
            consecutive_deasserted: 0,
            latched: false,
            resolved: false,
        }
    }

    fn clear_latch(&mut self) {
        self.latched = false;
        self.consecutive_asserted = 0;
        self.consecutive_deasserted = 0;
        self.resolved = false;
    }

    /// Returns whether a momentary discrete fired this frame.
    fn observe(&mut self, spec: CriticalDiscrete, sample: DualSample) -> bool {
        self.resolved = resolve_sample(spec, sample);
        if self.resolved {
            self.consecutive_asserted = self.consecutive_asserted.saturating_add(1);
            self.consecutive_deasserted = 0;
        } else {
            self.consecutive_deasserted = self.consecutive_deasserted.saturating_add(1);
            self.consecutive_asserted = 0;
        }

        match spec.kind() {
            DiscreteKind::Latched => {
                if self.consecutive_asserted >= spec.confirm_frames() {
                    self.latched = true;
                }
                if spec.drops_on_deassert() && self.consecutive_deasserted >= spec.confirm_frames()
                {
                    self.latched = false;
                }
                false
            }
            DiscreteKind::Momentary => self.consecutive_asserted == spec.confirm_frames(),
        }
    }
}

/// Inputs one control frame presents to the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafetyInputs {
    pub discretes: DiscreteFrame,
    pub heartbeat: Heartbeat,
    /// Caller's software-BIT sample this frame. Not a hardware BIT.
    pub bit_clear: bool,
    /// [`crate::input_health`] for this frame. Feeds Degraded; does not replace it.
    pub loop_health: Health,
}

impl SafetyInputs {
    /// Healthy frame, fail-safe discretes, heartbeat present. Test helper and
    /// the default a powered-on vehicle should look like before anyone arms.
    #[must_use]
    pub const fn healthy() -> Self {
        Self {
            discretes: DiscreteFrame::fail_safe(),
            heartbeat: Heartbeat::PRESENT,
            bit_clear: true,
            loop_health: Health::Nominal,
        }
    }
}

/// Mode manager plus discrete filters plus watchdog.
///
/// `Copy`, fixed size, no heap. Construct at rest in [`FlightMode::Startup`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafetyKernel {
    mode: FlightMode,
    watchdog: Watchdog,
    abort: DiscreteFilter,
    emergency: DiscreteFilter,
    master_arm: DiscreteFilter,
    reset_bit: DiscreteFilter,
    bit_ok_run: u8,
    bit_frames: u8,
    emergency_frames: u8,
}

impl SafetyKernel {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mode: FlightMode::Startup,
            watchdog: Watchdog::new(),
            abort: DiscreteFilter::new(),
            emergency: DiscreteFilter::new(),
            master_arm: DiscreteFilter::new(),
            reset_bit: DiscreteFilter::new(),
            bit_ok_run: 0,
            bit_frames: 0,
            emergency_frames: 0,
        }
    }

    #[must_use]
    pub const fn mode(self) -> FlightMode {
        self.mode
    }

    #[must_use]
    pub const fn watchdog(self) -> Watchdog {
        self.watchdog
    }

    #[must_use]
    pub const fn gate(self) -> ControlGate {
        ControlGate {
            mode: self.mode,
            authority: SurfaceAuthority::for_mode(self.mode),
            abort_latched: self.abort.latched,
            emergency_latched: self.emergency.latched,
            master_arm_latched: self.master_arm.latched,
            watchdog: self.watchdog.level(),
        }
    }

    /// One control frame. Never panics. Unknown / illegal combinations stay
    /// or take the documented fail-safe; they do not skip ahead.
    pub fn tick(&mut self, inputs: SafetyInputs) -> ControlGate {
        let wd = self.watchdog.tick(inputs.heartbeat);

        let abort_pulse = self
            .abort
            .observe(CriticalDiscrete::Abort, inputs.discretes.abort);
        let emergency_pulse = self
            .emergency
            .observe(CriticalDiscrete::Emergency, inputs.discretes.emergency);
        let arm_pulse = self
            .master_arm
            .observe(CriticalDiscrete::MasterArm, inputs.discretes.master_arm);
        let reset_pulse = self
            .reset_bit
            .observe(CriticalDiscrete::ResetBit, inputs.discretes.reset_bit);
        let _ = (abort_pulse, emergency_pulse, arm_pulse);

        if self.abort.latched || self.emergency.latched {
            self.master_arm.latched = false;
        }

        let event = self.select_event(inputs, wd, reset_pulse);
        let prev = self.mode;
        if let Some(event) = event {
            self.mode = prev.apply(event);
            if matches!(event, ModeEvent::ResetBit) && self.mode == FlightMode::Bit {
                self.on_reset_bit();
            }
        }

        if self.mode == FlightMode::Emergency {
            if prev == FlightMode::Emergency {
                self.emergency_frames = self.emergency_frames.saturating_add(1);
            } else {
                self.emergency_frames = 1;
            }
        }

        if self.mode == FlightMode::Bit {
            if prev != FlightMode::Bit {
                self.bit_ok_run = 0;
                self.bit_frames = 0;
            }
            self.bit_frames = self.bit_frames.saturating_add(1);
            if inputs.bit_clear
                && inputs.heartbeat.present
                && matches!(inputs.loop_health, Health::Nominal)
                && !self.abort.latched
                && !self.emergency.latched
            {
                self.bit_ok_run = self.bit_ok_run.saturating_add(1);
            } else {
                self.bit_ok_run = 0;
            }
        }

        self.gate()
    }

    fn on_reset_bit(&mut self) {
        self.abort.clear_latch();
        self.emergency.clear_latch();
        self.master_arm.clear_latch();
        self.reset_bit.clear_latch();
        self.watchdog.reset();
        self.bit_ok_run = 0;
        self.bit_frames = 0;
        self.emergency_frames = 0;
    }

    fn select_event(
        &self,
        inputs: SafetyInputs,
        wd: WatchdogLevel,
        reset_pulse: bool,
    ) -> Option<ModeEvent> {
        let aborting = self.abort.latched || self.emergency.latched;
        let loop_bad = !matches!(inputs.loop_health, Health::Nominal);
        let unhealthy = loop_bad || matches!(wd, WatchdogLevel::Degraded | WatchdogLevel::TripSafe);

        if aborting {
            return match self.mode {
                FlightMode::Safe => {
                    if reset_pulse
                        && !self.abort.resolved
                        && !self.emergency.resolved
                        && !self.master_arm.resolved
                    {
                        Some(ModeEvent::ResetBit)
                    } else {
                        None
                    }
                }
                FlightMode::Emergency => {
                    if self.emergency_frames >= EMERGENCY_TO_SAFE_FRAMES {
                        Some(ModeEvent::EnterSafe)
                    } else {
                        None
                    }
                }
                FlightMode::Startup
                | FlightMode::Bit
                | FlightMode::Ready
                | FlightMode::Nominal
                | FlightMode::Degraded => {
                    if self.emergency.latched && !self.abort.latched {
                        Some(ModeEvent::DeclareEmergency)
                    } else {
                        Some(ModeEvent::Abort)
                    }
                }
            };
        }

        match self.mode {
            FlightMode::Startup => Some(ModeEvent::BeginBit),
            FlightMode::Bit => {
                if matches!(wd, WatchdogLevel::TripSafe) || self.bit_frames >= BIT_TIMEOUT_FRAMES {
                    Some(ModeEvent::BitFail)
                } else if self.bit_ok_run >= BIT_PASS_FRAMES {
                    Some(ModeEvent::BitPass)
                } else if reset_pulse {
                    Some(ModeEvent::ResetBit)
                } else {
                    None
                }
            }
            FlightMode::Ready => {
                if matches!(wd, WatchdogLevel::TripSafe) {
                    Some(ModeEvent::EnterSafe)
                } else if unhealthy {
                    Some(ModeEvent::Degrade)
                } else if self.master_arm.latched {
                    Some(ModeEvent::Arm)
                } else {
                    None
                }
            }
            FlightMode::Nominal => {
                if matches!(wd, WatchdogLevel::TripSafe) {
                    Some(ModeEvent::EnterSafe)
                } else if unhealthy {
                    Some(ModeEvent::Degrade)
                } else if !self.master_arm.latched {
                    Some(ModeEvent::Disarm)
                } else {
                    None
                }
            }
            FlightMode::Degraded => {
                if matches!(wd, WatchdogLevel::TripSafe) {
                    Some(ModeEvent::EnterSafe)
                } else if !unhealthy && self.master_arm.latched {
                    Some(ModeEvent::Recover)
                } else if !unhealthy && !self.master_arm.latched {
                    Some(ModeEvent::Disarm)
                } else {
                    None
                }
            }
            FlightMode::Emergency => {
                if self.emergency_frames >= EMERGENCY_TO_SAFE_FRAMES {
                    Some(ModeEvent::EnterSafe)
                } else {
                    None
                }
            }
            FlightMode::Safe => {
                if reset_pulse
                    && !self.abort.resolved
                    && !self.emergency.resolved
                    && !self.master_arm.resolved
                {
                    Some(ModeEvent::ResetBit)
                } else {
                    None
                }
            }
        }
    }
}

impl Default for SafetyKernel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Health;

    fn healthy_with(discretes: DiscreteFrame) -> SafetyInputs {
        SafetyInputs {
            discretes,
            ..SafetyInputs::healthy()
        }
    }

    fn arm_frame() -> DiscreteFrame {
        DiscreteFrame {
            master_arm: DualSample::both(true),
            ..DiscreteFrame::fail_safe()
        }
    }

    fn abort_frame() -> DiscreteFrame {
        DiscreteFrame {
            abort: DualSample::both(true),
            ..DiscreteFrame::fail_safe()
        }
    }

    fn drive_to_ready(k: &mut SafetyKernel) {
        // Startup → Bit, then BIT_PASS_FRAMES good Bit frames, then one more
        // tick to apply BitPass. Bound is well under BIT_TIMEOUT_FRAMES.
        for _ in 0..(BIT_PASS_FRAMES as u16 + 4) {
            if k.mode() == FlightMode::Ready {
                return;
            }
            let _ = k.tick(SafetyInputs::healthy());
        }
        panic!("did not reach Ready; stuck in {:?}", k.mode());
    }

    fn drive_to_nominal(k: &mut SafetyKernel) {
        drive_to_ready(k);
        let armed = healthy_with(arm_frame());
        for _ in 0..(DISCRETE_CONFIRM_FRAMES as u16 + 2) {
            if k.mode() == FlightMode::Nominal {
                return;
            }
            let _ = k.tick(armed);
        }
        panic!("did not reach Nominal; stuck in {:?}", k.mode());
    }

    /// TOTALITY. Every mode × event pair produces a defined mode. A table that
    /// can panic is not a mode table.
    #[test]
    fn every_mode_event_pair_is_total() {
        for &mode in &FlightMode::ALL {
            for &event in &ModeEvent::ALL {
                let next = mode.apply(event);
                assert!(
                    FlightMode::ALL.contains(&next),
                    "{mode:?} + {event:?} left the enum"
                );
            }
        }
    }

    /// Illegal transitions stay. These would be the silent skips: arm from
    /// Safe, pass BIT from Startup, reset from Nominal into Bit.
    #[test]
    fn illegal_transitions_stay() {
        assert_eq!(FlightMode::Safe.apply(ModeEvent::Arm), FlightMode::Safe);
        assert_eq!(
            FlightMode::Safe.apply(ModeEvent::BeginBit),
            FlightMode::Safe
        );
        assert_eq!(FlightMode::Safe.apply(ModeEvent::Recover), FlightMode::Safe);
        assert_eq!(
            FlightMode::Startup.apply(ModeEvent::BitPass),
            FlightMode::Startup
        );
        assert_eq!(
            FlightMode::Startup.apply(ModeEvent::Arm),
            FlightMode::Startup
        );
        assert_eq!(FlightMode::Bit.apply(ModeEvent::Arm), FlightMode::Bit);
        assert_eq!(
            FlightMode::Nominal.apply(ModeEvent::BeginBit),
            FlightMode::Nominal
        );
        assert_eq!(
            FlightMode::Nominal.apply(ModeEvent::ResetBit),
            FlightMode::Nominal
        );
        assert_eq!(
            FlightMode::Nominal.apply(ModeEvent::BitPass),
            FlightMode::Nominal
        );
        assert_eq!(
            FlightMode::Ready.apply(ModeEvent::Recover),
            FlightMode::Ready
        );
        assert_eq!(
            FlightMode::Emergency.apply(ModeEvent::Arm),
            FlightMode::Emergency
        );
        assert_eq!(
            FlightMode::Emergency.apply(ModeEvent::ResetBit),
            FlightMode::Emergency
        );
        assert_eq!(
            FlightMode::Degraded.apply(ModeEvent::BeginBit),
            FlightMode::Degraded
        );
    }

    #[test]
    fn legal_transitions_are_the_documented_ones() {
        assert_eq!(
            FlightMode::Startup.apply(ModeEvent::BeginBit),
            FlightMode::Bit
        );
        assert_eq!(FlightMode::Bit.apply(ModeEvent::BitPass), FlightMode::Ready);
        assert_eq!(FlightMode::Bit.apply(ModeEvent::BitFail), FlightMode::Safe);
        assert_eq!(FlightMode::Ready.apply(ModeEvent::Arm), FlightMode::Nominal);
        assert_eq!(
            FlightMode::Ready.apply(ModeEvent::Degrade),
            FlightMode::Degraded
        );
        assert_eq!(
            FlightMode::Nominal.apply(ModeEvent::Disarm),
            FlightMode::Ready
        );
        assert_eq!(
            FlightMode::Nominal.apply(ModeEvent::Degrade),
            FlightMode::Degraded
        );
        assert_eq!(
            FlightMode::Degraded.apply(ModeEvent::Recover),
            FlightMode::Nominal
        );
        assert_eq!(
            FlightMode::Degraded.apply(ModeEvent::Disarm),
            FlightMode::Ready
        );
        assert_eq!(FlightMode::Safe.apply(ModeEvent::ResetBit), FlightMode::Bit);
        assert_eq!(
            FlightMode::Nominal.apply(ModeEvent::Abort),
            FlightMode::Emergency
        );
        assert_eq!(FlightMode::Safe.apply(ModeEvent::Abort), FlightMode::Safe);
        assert_eq!(
            FlightMode::Bit.apply(ModeEvent::EnterSafe),
            FlightMode::Safe
        );
    }

    /// Wire tags: every byte maps to a mode or to None. No panic, no default
    /// to Startup for an unknown tag (that would be a silent mode skip).
    #[test]
    fn mode_wire_tags_are_total_over_u8() {
        let mut known = 0u8;
        for raw in 0u8..=255 {
            match FlightMode::from_wire(raw) {
                Some(mode) => {
                    assert_eq!(mode.to_wire(), raw);
                    known += 1;
                }
                None => assert!(raw > 6, "gap in assigned tags at {raw}"),
            }
        }
        assert_eq!(known, FlightMode::ALL.len() as u8);
    }

    /// Garbage discrete words never panic and never arm from Startup.
    #[test]
    fn garbage_wire_frames_do_not_panic_or_arm() {
        for bits in 0u16..=255 {
            let mut k = SafetyKernel::new();
            let inputs = healthy_with(DiscreteFrame::from_wire(bits));
            let gate = k.tick(inputs);
            assert!(!gate.allows_surface_command());
            assert_ne!(gate.mode, FlightMode::Nominal);
        }
    }

    #[test]
    fn power_on_walks_startup_bit_ready() {
        let mut k = SafetyKernel::new();
        assert_eq!(k.mode(), FlightMode::Startup);
        assert!(!k.gate().allows_surface_command());
        drive_to_ready(&mut k);
        assert_eq!(k.mode(), FlightMode::Ready);
        assert!(!k.gate().allows_surface_command());
    }

    #[test]
    fn master_arm_requires_dual_agreement_and_confirm() {
        let mut k = SafetyKernel::new();
        drive_to_ready(&mut k);

        let split = healthy_with(DiscreteFrame {
            master_arm: DualSample::split(true, false),
            ..DiscreteFrame::fail_safe()
        });
        for _ in 0..8 {
            let gate = k.tick(split);
            assert_eq!(gate.mode, FlightMode::Ready, "split arm must refuse");
            assert!(!gate.master_arm_latched);
            assert!(!gate.allows_surface_command());
        }

        let one = healthy_with(arm_frame());
        let g1 = k.tick(one);
        assert_eq!(g1.mode, FlightMode::Ready, "one sample is not a confirm");
        let g2 = k.tick(one);
        assert_eq!(g2.mode, FlightMode::Nominal);
        assert!(g2.master_arm_latched);
        assert!(g2.allows_surface_command());
    }

    #[test]
    fn master_arm_drops_after_confirmed_deassert() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let off = SafetyInputs::healthy();
        let _ = k.tick(off);
        assert_eq!(k.mode(), FlightMode::Nominal, "one deassert is not a drop");
        let g = k.tick(off);
        assert_eq!(g.mode, FlightMode::Ready);
        assert!(!g.master_arm_latched);
        assert!(!g.allows_surface_command());
    }

    /// THE SILENT-DROP TEST. If abort handling is deleted, this fails: two
    /// confirmed dual-assert frames must leave Nominal and freeze surfaces.
    #[test]
    fn abort_assert_is_not_dropped_on_the_floor() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        assert!(k.gate().allows_surface_command());

        let aborting = healthy_with(abort_frame());
        let g1 = k.tick(aborting);
        assert_eq!(
            g1.mode,
            FlightMode::Nominal,
            "debounce: one abort sample must not yet take"
        );
        assert!(
            g1.allows_surface_command(),
            "still Nominal until confirm, so this is the frame a glitch is rejected"
        );

        let g2 = k.tick(aborting);
        assert_ne!(
            g2.mode,
            FlightMode::Nominal,
            "a confirmed abort must leave Nominal — if this fails, abort is being ignored"
        );
        assert_eq!(g2.mode, FlightMode::Emergency);
        assert!(g2.abort_latched);
        assert!(!g2.allows_surface_command());
        assert!(!g2.master_arm_latched, "abort clears effector arm");
    }

    #[test]
    fn abort_channel_disagreement_still_aborts() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let split = healthy_with(DiscreteFrame {
            abort: DualSample::split(true, false),
            ..DiscreteFrame::fail_safe()
        });
        let _ = k.tick(split);
        let g = k.tick(split);
        assert_eq!(
            g.mode,
            FlightMode::Emergency,
            "a split abort must not be flown through"
        );
        assert!(g.abort_latched);
    }

    #[test]
    fn emergency_detection_latches_into_safe() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let aborting = healthy_with(abort_frame());
        let _ = k.tick(aborting);
        let detect = k.tick(aborting);
        assert_eq!(detect.mode, FlightMode::Emergency);
        let latched = k.tick(aborting);
        assert_eq!(latched.mode, FlightMode::Safe);
        assert!(!latched.allows_surface_command());

        // Restored heartbeat does not leave Safe.
        for _ in 0..8 {
            let g = k.tick(SafetyInputs::healthy());
            assert_eq!(g.mode, FlightMode::Safe);
        }
    }

    #[test]
    fn reset_bit_from_safe_reenters_bit_only_when_lines_are_clear() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let aborting = healthy_with(abort_frame());
        let _ = k.tick(aborting);
        let _ = k.tick(aborting);
        let _ = k.tick(aborting);
        assert_eq!(k.mode(), FlightMode::Safe);

        let reset_while_held = healthy_with(DiscreteFrame {
            abort: DualSample::both(true),
            reset_bit: DualSample::both(true),
            ..DiscreteFrame::fail_safe()
        });
        for _ in 0..4 {
            let g = k.tick(reset_while_held);
            assert_eq!(
                g.mode,
                FlightMode::Safe,
                "ResetBIT ignored while abort held"
            );
        }

        // Momentary: the pulse was consumed while blocked. Release, then press.
        let clear = SafetyInputs::healthy();
        let _ = k.tick(clear);
        let _ = k.tick(clear);

        let reset = healthy_with(DiscreteFrame {
            reset_bit: DualSample::split(true, false), // B ignored: single-channel
            ..DiscreteFrame::fail_safe()
        });
        let _ = k.tick(reset);
        let g = k.tick(reset);
        assert_eq!(g.mode, FlightMode::Bit);
        assert!(!g.abort_latched);
        assert!(!g.allows_surface_command());
    }

    #[test]
    fn reset_bit_ignores_channel_b() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let aborting = healthy_with(abort_frame());
        let _ = k.tick(aborting);
        let _ = k.tick(aborting);
        let _ = k.tick(aborting);
        assert_eq!(k.mode(), FlightMode::Safe);

        let b_only = healthy_with(DiscreteFrame {
            reset_bit: DualSample::split(false, true),
            ..DiscreteFrame::fail_safe()
        });
        for _ in 0..6 {
            assert_eq!(k.tick(b_only).mode, FlightMode::Safe);
        }
    }

    #[test]
    fn watchdog_misses_degrade_then_trip_safe() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let mute = SafetyInputs {
            heartbeat: Heartbeat::MISSING,
            discretes: arm_frame(),
            ..SafetyInputs::healthy()
        };

        for i in 0..WATCHDOG_DEGRADED_MISSES {
            let g = k.tick(mute);
            if i + 1 < WATCHDOG_DEGRADED_MISSES {
                assert_eq!(g.mode, FlightMode::Nominal, "miss {}", i + 1);
            } else {
                assert_eq!(g.mode, FlightMode::Degraded);
                assert!(!g.allows_surface_command());
            }
        }

        let remaining = WATCHDOG_SAFE_MISSES - WATCHDOG_DEGRADED_MISSES;
        for _ in 0..remaining.saturating_sub(1) {
            let g = k.tick(mute);
            assert_eq!(g.mode, FlightMode::Degraded);
        }
        let g = k.tick(mute);
        assert_eq!(g.mode, FlightMode::Safe);
        assert!(!g.allows_surface_command());
    }

    #[test]
    fn watchdog_is_testable_without_the_kernel() {
        let mut w = Watchdog::new();
        assert_eq!(w.level(), WatchdogLevel::Ok);
        for _ in 0..WATCHDOG_DEGRADED_MISSES {
            w.tick(Heartbeat::MISSING);
        }
        assert_eq!(w.level(), WatchdogLevel::Degraded);
        for _ in 0..(WATCHDOG_SAFE_MISSES - WATCHDOG_DEGRADED_MISSES) {
            w.tick(Heartbeat::MISSING);
        }
        assert_eq!(w.level(), WatchdogLevel::TripSafe);
        w.tick(Heartbeat::PRESENT);
        assert_eq!(w.level(), WatchdogLevel::Ok);
        assert_eq!(w.consecutive_misses(), 0);
    }

    #[test]
    fn sensor_fault_degrades_an_armed_loop() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let bad = SafetyInputs {
            loop_health: Health::SensorFault,
            discretes: arm_frame(),
            ..SafetyInputs::healthy()
        };
        let g = k.tick(bad);
        assert_eq!(g.mode, FlightMode::Degraded);
        assert!(!g.allows_surface_command());

        let good = healthy_with(arm_frame());
        // Arm is still latched; one good frame recovers.
        let g = k.tick(good);
        assert_eq!(g.mode, FlightMode::Nominal);
        assert!(g.allows_surface_command());
    }

    #[test]
    fn bit_timeout_goes_safe() {
        let mut k = SafetyKernel::new();
        let _ = k.tick(SafetyInputs::healthy()); // Startup → Bit
        assert_eq!(k.mode(), FlightMode::Bit);
        let stuck = SafetyInputs {
            bit_clear: false,
            ..SafetyInputs::healthy()
        };
        for _ in 0..BIT_TIMEOUT_FRAMES {
            let _ = k.tick(stuck);
            if k.mode() == FlightMode::Safe {
                break;
            }
        }
        assert_eq!(k.mode(), FlightMode::Safe);
    }

    #[test]
    fn cannot_arm_from_safe() {
        let mut k = SafetyKernel::new();
        drive_to_nominal(&mut k);
        let aborting = healthy_with(abort_frame());
        let _ = k.tick(aborting);
        let _ = k.tick(aborting);
        let _ = k.tick(aborting);
        assert_eq!(k.mode(), FlightMode::Safe);

        let armed = healthy_with(arm_frame());
        for _ in 0..10 {
            let g = k.tick(armed);
            assert_eq!(g.mode, FlightMode::Safe);
            assert!(!g.allows_surface_command());
        }
    }

    /// DETERMINISM. Integer/bool kernel, same inputs, identical gate.
    #[test]
    fn kernel_is_bit_deterministic() {
        let run = || {
            let mut k = SafetyKernel::new();
            drive_to_nominal(&mut k);
            let aborting = healthy_with(abort_frame());
            let _ = k.tick(aborting);
            let _ = k.tick(aborting);
            k.gate()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn fail_safe_defaults_are_deasserted() {
        for d in [
            CriticalDiscrete::Abort,
            CriticalDiscrete::Emergency,
            CriticalDiscrete::MasterArm,
            CriticalDiscrete::ResetBit,
        ] {
            assert!(!d.fail_safe_asserted());
            assert_eq!(d.confirm_frames(), DISCRETE_CONFIRM_FRAMES);
        }
        assert_eq!(
            CriticalDiscrete::ResetBit.channel_policy(),
            ChannelPolicy::Single
        );
        assert_eq!(
            CriticalDiscrete::MasterArm.on_disagree(),
            DisagreeAction::Deassert
        );
        assert_eq!(
            CriticalDiscrete::Abort.on_disagree(),
            DisagreeAction::Assert
        );
    }

    #[test]
    fn resolve_sample_matches_policy() {
        let on = DualSample::both(true);
        let off = DualSample::DEASSERTED;
        let split = DualSample::split(true, false);
        assert!(resolve_sample(CriticalDiscrete::Abort, on));
        assert!(!resolve_sample(CriticalDiscrete::Abort, off));
        assert!(resolve_sample(CriticalDiscrete::Abort, split));
        assert!(!resolve_sample(CriticalDiscrete::MasterArm, split));
        assert!(resolve_sample(
            CriticalDiscrete::ResetBit,
            DualSample::split(true, false)
        ));
        assert!(!resolve_sample(
            CriticalDiscrete::ResetBit,
            DualSample::split(false, true)
        ));
    }

    /// The eight-bit packing is bijective. HIL byte 0 is this function
    /// (ADR-004 D6); a silent remap would make abort look deasserted.
    #[test]
    fn discrete_frame_wire_packing_round_trips_eight_bits() {
        for bits in 0u16..=255 {
            let frame = DiscreteFrame::from_wire(bits);
            assert_eq!(frame.to_wire() as u16, bits);
            assert_eq!(DiscreteFrame::from_wire(frame.to_wire() as u16), frame);
        }
        assert_eq!(
            DiscreteFrame::from_wire(0x0100),
            DiscreteFrame::from_wire(0),
            "high bits must be ignored, never panic or arm"
        );
    }

    #[test]
    fn authority_and_watchdog_wire_tags_are_total_over_u8() {
        let mut known_auth = 0u8;
        let mut known_wd = 0u8;
        for raw in 0u8..=255 {
            match SurfaceAuthority::from_wire(raw) {
                Some(a) => {
                    assert_eq!(a.to_wire(), raw);
                    known_auth += 1;
                }
                None => assert!(raw > 1, "gap in authority tags at {raw}"),
            }
            match WatchdogLevel::from_wire(raw) {
                Some(w) => {
                    assert_eq!(w.to_wire(), raw);
                    known_wd += 1;
                }
                None => assert!(raw > 2, "gap in watchdog tags at {raw}"),
            }
        }
        assert_eq!(known_auth, 2);
        assert_eq!(known_wd, 3);
        assert_eq!(SurfaceAuthority::HoldLast.to_wire(), 0);
        assert_eq!(SurfaceAuthority::Command.to_wire(), 1);
    }
}
