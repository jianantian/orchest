use agent_runtime_providers::catalog::list_models;

fn main() {
    for m in list_models().filter(|m| m.provider == "volcengine") {
        println!("─────────────────────────────────────────");
        println!("model_id      : {}", m.model_id);
        println!("display_name  : {}", m.display_name);
        println!("description   : {}", m.description);
        println!("context_window: {}", m.context_window);
        println!("max_input     : {:?}", m.max_input_tokens);
        println!("max_output    : {:?}", m.max_output_tokens);
        println!("thinking      : {:?}", m.thinking);
        println!("input_modes   : {:?}", m.input_modalities);
        println!("output_modes  : {:?}", m.output_modalities);
        println!("scenes        : {:?}", m.scenes);
        if let Some(p) = &m.pricing {
            println!("pricing       : {} ({} tier(s))", p.currency, p.tiers.len());
            for (i, tier) in p.tiers.iter().enumerate() {
                let cap = tier
                    .max_input_tokens
                    .map_or_else(|| "∞".to_string(), |c| format!("≤{}", c));
                let r = &tier.rates;
                let mut line = format!(
                    "  tier[{}] input {:<8} text in={} out={}",
                    i, cap, r.text_input_per_million, r.text_output_per_million,
                );
                if let Some(v) = r.audio_input_per_million {
                    line.push_str(&format!(" audio_in={}", v));
                }
                if let Some(v) = r.image_input_per_million {
                    line.push_str(&format!(" image_in={}", v));
                }
                if let Some(v) = r.video_input_per_million {
                    line.push_str(&format!(" video_in={}", v));
                }
                if let Some(v) = r.cache_read_per_million {
                    line.push_str(&format!(" cache_read={}", v));
                }
                if let Some(v) = r.cache_write_per_million {
                    line.push_str(&format!(" cache_write={}", v));
                }
                println!("{line}");
            }
        }
    }
}
