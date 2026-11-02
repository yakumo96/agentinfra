//! Compile-fail fixture: a running handle cannot be started again.

use infra_telemetry::timer::{Active, Timing, TimingScope};

fn main() {
    let (_result, _report) = Timing::record(
        "op",
        |root: &TimingScope<'_, Active>| -> Result<(), ()> { root.run(|_| Ok(())) },
    );
}
