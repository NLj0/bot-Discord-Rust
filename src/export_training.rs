mod selfbot_data;

use selfbot_data::TrainingPair;
use serde_json::json;
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let pairs_path = PathBuf::from(&data_dir).join("clean/training_pairs.jsonl");
    let output_path = PathBuf::from(&data_dir).join("clean/training_export.jsonl");

    let raw = tokio::fs::read_to_string(&pairs_path)
        .await
        .expect("training_pairs.jsonl not found");
    let pairs: Vec<TrainingPair> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("invalid training pair line"))
        .collect();

    let mut exported = 0;
    let mut lines = Vec::new();

    for pair in pairs {
        let record = json!({
            "instruction": pair.input,
            "output": pair.output,
            "metadata": {
                "input_author": pair.input_author,
                "output_author": pair.output_author,
                "message_id": pair.message_id,
                "quality_score": pair.quality_score,
            }
        });
        lines.push(serde_json::to_string(&record).expect("serialize export row"));
        exported += 1;
    }

    let body = lines.join("\n");
    tokio::fs::write(&output_path, format!("{body}\n"))
        .await
        .expect("failed to write export file");

    println!("Exported {exported} training pairs to {}", output_path.display());
}
