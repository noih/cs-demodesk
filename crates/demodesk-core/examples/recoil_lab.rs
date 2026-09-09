//! Export shot-level evidence from a controlled recoil recording.
use demodesk_core::parser::DemoParser;
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(windows)]
    if args.get(1).is_some_and(|arg| arg == "--window-hook") {
        std::fs::write(args.get(2).expect("DLL output path"), include_bytes!(concat!(env!("OUT_DIR"), "/demodesk-window-hook.dll")))?;
        return Ok(());
    }
    let bytes = std::fs::read(args.get(1).expect("demo path"))?;
    let parser = DemoParser::new();
    let names = ["weapon_fire", "fire_bullets", "bullet_impact"];
    let props = ["X", "Y", "Z", "pitch", "yaw", "ducking", "ducked", "duck_amount", "is_alive", "active_weapon_name", "fl_recoil_idx", "aim_punch_angle"];
    let events = parser.events(&bytes, &names.map(String::from), &props.map(String::from), &[])?;
    let data = serde_json::json!({"header": parser.header(&bytes)?, "events": events.game_events});
    std::fs::write(args.get(2).expect("output JSON path"), serde_json::to_vec_pretty(&data)?)?;
    Ok(())
}
