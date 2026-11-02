//! Production filesystem daemon entry; command execution belongs to the runtime.
fn main() -> std::process::ExitCode {
    infra_sandboxd::application::main()
}
