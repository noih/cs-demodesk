//! Inspect original flattened sendtable ownership, before parser field naming.
use anyhow::{anyhow, ensure, Result};
use parser::first_pass::parser_settings::{FirstPassParser, ParserInputs};
use parser::second_pass::parser_settings::create_huffman_lookup_table;
use prost::Message;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 1, "expected DEMO");
    let bytes = std::fs::read(&args[0])?;
    let huffman = create_huffman_lookup_table();
    let inputs = ParserInputs {
        real_name_to_og_name: Default::default(),
        wanted_players: vec![],
        wanted_player_props: vec![],
        wanted_other_props: vec![],
        wanted_prop_states: Default::default(),
        wanted_ticks: vec![i32::MIN],
        wanted_events: vec![],
        parse_ents: true,
        parse_projectiles: false,
        parse_grenades: false,
        only_header: false,
        only_convars: false,
        huffman_lookup_table: &huffman,
        order_by_steamid: false,
        list_props: false,
        fallback_bytes: None,
    };
    let mut parser = FirstPassParser::new(&inputs);

    let _ = parser
        .parse_demo(&bytes, true)
        .map_err(|e| anyhow!("{e:?}"))?;
    let table = parser
        .sendtable_message
        .as_ref()
        .ok_or_else(|| anyhow!("missing sendtable"))?;
    let mut reader = parser::first_pass::read_bits::Bitreader::new(table.data());
    let count = reader.read_varint().map_err(|e| anyhow!("{e:?}"))?;
    let raw = reader
        .read_n_bytes(count as usize)
        .map_err(|e| anyhow!("{e:?}"))?;
    let message = csgoproto::CsvcMsgFlattenedSerializer::decode(raw.as_slice())?;
    let sym = |id: Option<i32>| id.and_then(|i| message.symbols.get(i as usize)).cloned();
    let mut found = vec![];
    for serializer in &message.serializers {
        let owner = sym(serializer.serializer_name_sym).unwrap_or_default();
        if owner != "CCSPlayerPawn" && owner != "CBodyComponentBaseAnimGraph" {
            continue;
        }
        for (position, index) in serializer.fields_index.iter().enumerate() {
            let field = &message.fields[*index as usize];
            let name = sym(field.var_name_sym).unwrap_or_default();
            if ![
                "m_vecX",
                "m_vecY",
                "m_vecZ",
                "m_vecViewOffset",
                "m_primaryGraphId",
                "m_hGraphDefinitionAG2",
                "m_hModel",
                "m_vecSecondarySkeletons",
                "m_hSkeletonDefinitionAG2",
            ]
            .contains(&name.as_str())
            {
                continue;
            }
            found.push(serde_json::json!({
                "owner":owner, "position":position,"fieldIndex":index,"name":name,
                "type":sym(field.var_type_sym),"sendNode":sym(field.send_node_sym),
                "serializer":sym(field.field_serializer_name_sym),"encoder":sym(field.var_encoder_sym),
                "bits":field.bit_count,"low":field.low_value,"high":field.high_value
            }));
        }
    }
    println!("{}", serde_json::to_string_pretty(&found)?);
    Ok(())
}
