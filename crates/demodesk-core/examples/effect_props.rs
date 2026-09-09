use parser::first_pass::parser_settings::{FirstPassParser, ParserInputs};
use parser::second_pass::parser_settings::create_huffman_lookup_table;
fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("demo path");
    let bytes = std::fs::read(path)?;
    let huffman = create_huffman_lookup_table();
    let inputs = ParserInputs {
        real_name_to_og_name: Default::default(), wanted_players: vec![], wanted_player_props: vec![], wanted_other_props: vec![],
        wanted_prop_states: Default::default(), wanted_ticks: vec![], wanted_events: vec![], parse_ents: true, parse_projectiles: true,
        parse_grenades: true, only_header: false, only_convars: false, huffman_lookup_table: &huffman, order_by_steamid: false,
        list_props: true, fallback_bytes: None,
    };
    let mut parser = FirstPassParser::new(&inputs);
    let out = parser.parse_demo(&bytes, true).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut names: Vec<_> = out.prop_controller.name_to_id.keys().filter(|n| {
        let n = n.to_lowercase(); n.contains("fire") || n.contains("smoke") || n.contains("voxel") || n.contains("inferno")
    }).collect();
    names.sort();
    for name in names { println!("{name}"); }
    let rows = demodesk_core::parser::DemoParser::new().projectiles(&bytes, (0..160000).step_by(64).collect())?;
    let fires: Vec<_> = rows.iter().filter(|r| r.str("grenade_type") == Some("CInferno")).collect();
    println!("inferno rows: {}, active cells: {}", fires.len(), fires.iter().filter(|r| r.strs("m_firePositions").is_some_and(|p| !p.is_empty())).count());
    for row in fires.iter().filter(|r| r.strs("m_firePositions").is_some_and(|p| !p.is_empty())).take(3) {
        println!("tick {:?}, count {:?}, type {:?}, cells {:?}", row.tick(), row.num("m_fireCount"), row.num("m_nInfernoType"), row.strs("m_firePositions"));
    }
    println!("smoke voxel max size: {}", rows.iter().filter_map(|r| r.num("m_nVoxelFrameDataSize")).fold(0.0_f64, f64::max));
    Ok(())
}
