//! Inspect a version-2 recipe prefix; this does not reconstruct body geometry.
use anyhow::{ensure, Context as _, Result};
use demodesk_core::analysis::animation_recipe::{decode, index_bits, Context, RECORDED_TASK_NAMES};

fn hex(value: &str) -> Result<Vec<u8>> {
    ensure!(
        value.len().is_multiple_of(2) && value.is_ascii(),
        "expected hexadecimal bytes"
    );
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 4,
        "expected TOPOLOGY_HEX DYNAMIC_HEX RESOURCE_COUNT MASK_COUNT"
    );
    let resources: u32 = args[2].parse().context("invalid resource count")?;
    let masks: u32 = args[3].parse().context("invalid mask count")?;
    let context = Context {
        task_names: &RECORDED_TASK_NAMES,
        resource_count: resources,
        resource_bits: index_bits(resources),
        mask_count: masks,
        mask_bits: index_bits(masks),
        bone_names: &[],
    };
    let recipe = decode(2, &hex(&args[0])?, &hex(&args[1])?, &context)?;
    println!(
        "network tick: {}; tasks: {}; consumed bits: {}; unparsed bits: {}",
        recipe.network_tick,
        recipe.tasks.len(),
        recipe.bits_consumed,
        recipe.remaining_bits()
    );
    for (task, parameters) in recipe.tasks.iter().zip(&recipe.parameters) {
        println!("{task:?}: {parameters:?}");
    }
    Ok(())
}
