//! Binary `ezagent-voice`: mismo arranque que `python -m ezagent.voice.app`.

fn main() {
    std::process::exit(ez_app::app::main::main());
}
