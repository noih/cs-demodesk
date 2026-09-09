use demodesk_core::parser::DemoParser;
fn main() {
 let args:Vec<String>=std::env::args().collect();
 let bytes=std::fs::read(&args[1]).unwrap();
 let names=["fire_bullets","bullet_impact","player_hurt","player_death","weapon_fire","player_blind","flashbang_detonate","hegrenade_detonate","smokegrenade_detonate","inferno_startburn"];
 let props=["health","team_num","team_name","active_weapon_name","pitch","yaw","aim_punch_angle","shots_fired","fl_recoil_idx"];
 let out=DemoParser::new().events(&bytes,&names.map(String::from),&props.map(String::from),&["is_freeze_period".into()]).unwrap();
 std::fs::write(&args[2],serde_json::to_string(&out.game_events).unwrap()).unwrap();
}
