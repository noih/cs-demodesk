//! Inspect recorded animation contexts without running the game or any scoring rule.
use anyhow::{anyhow, ensure, Result};
use parser::first_pass::parser_settings::{FirstPassParser, ParserInputs};
use parser::second_pass::parser_settings::{create_huffman_lookup_table, SecondPassParser};
use std::{collections::BTreeMap, io::Write};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        (2..=3).contains(&args.len()),
        "expected DEMO NEW_OUTPUT_JSON [LAST_TICK]"
    );
    let last_tick = args
        .get(2)
        .map(|n| n.parse::<i32>())
        .transpose()?
        .unwrap_or(i32::MAX);
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
    let first = parser
        .parse_demo(&bytes, true)
        .map_err(|e| anyhow!("{e:?}"))?;
    let mut second = SecondPassParser::new(
        first,
        parser::first_pass::parser::HEADER_ENDS_AT_BYTE,
        true,
        None,
    )
    .map_err(|e| anyhow!("{e:?}"))?;
    second.capture_pose_fields = true;
    second.analysis_changes = Some(Default::default());
    let mut packets = 0u64;
    let mut final_tick = None;
    second
        .start_with_observer(&bytes, |p| {
            packets += 1;
            final_tick = Some(p.tick);
            p.tick < last_tick
        })
        .map_err(|e| anyhow!("{e:?}"))?;
    let mut context = BTreeMap::new();
    for table in &second.animation_strings.tables {
        for (index, (key, bytes)) in &table.entries {
            let mut name = vec![5u8];
            name.extend(key.as_bytes());
            let mut data = vec![14u8];
            data.extend(bytes);
            context.insert(format!("{}/{index}/name", table.name), name);
            context.insert(format!("{}/{index}/data", table.name), data);
        }
    }
    let context_digest = sha1_smol::Sha1::from(serde_json::to_vec(&context)?)
        .digest()
        .to_string();
    let model_entities = second
        .entities
        .iter()
        .flatten()
        .filter_map(|entity| {
            let properties = entity
                .props
                .iter()
                .filter_map(|(id, value)| {
                    let name = second.prop_controller.id_to_name.get(id)?;
                    (name.ends_with(".m_hModel")
                        || name.ends_with(".m_hGraphDefinitionAG2")
                        || name.ends_with(".m_hSkeletonDefinitionAG2"))
                    .then(|| (name.clone(), value.clone()))
                })
                .collect::<BTreeMap<_, _>>();
            (!properties.is_empty()).then(|| {
                serde_json::json!({"entity":entity.entity_id,"serial":entity.serial,
            "class":second.cls_by_id[entity.cls_id as usize].name,"properties":properties})
            })
        })
        .collect::<Vec<_>>();
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    serde_json::to_writer(
        &mut out,
        &serde_json::json!({"baselineComparison":second.cls_by_id.iter().enumerate().filter_map(|(id, cls)| { let actual=second.animation_strings.instance_baseline(id as u32)?; let legacy=second.baselines.get(&(id as u32)); Some(serde_json::json!({"id":id,"class":cls.name,"wireBytes":actual.len(),"legacyBytes":legacy.map(|b|b.len()),"equal":legacy.map(|b|b.as_slice())==Some(actual)})) }).collect::<Vec<_>>(),"modelEntities":model_entities,"contextDigest":context_digest,"contextFields":context.len(),"packets":packets,"lastTick":final_tick,"nativeTables":second.animation_strings.tables.iter().filter(|t| !t.entries.is_empty()).map(|t| serde_json::json!({"name":t.name,"entries":t.entries})).collect::<Vec<_>>()}),
    )?;
    out.flush()?;
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"contextDigest":context_digest,"contextFields":context.len(),"packets":packets,"lastTick":final_tick})
        )?
    );
    Ok(())
}
