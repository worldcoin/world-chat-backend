use anyhow::Result;
use crypto_box::{aead::OsRng, PublicKey};
use csv::Writer;
use e2e_tests::proof::{generate_proof, AuthRequest};
use std::fs::File;

const NUM_USERS: usize = 10;
const APP_ID: &str = "worldchat-chat-backend-staging";
const ACTION: &str = "authorize";
const OUTPUT_FILE: &str = "user_data.csv";

#[tokio::main]
async fn main() -> Result<()> {
    println!("Generating test data for {} users...", NUM_USERS);
    println!("This may take a few minutes due to proof generation...\n");

    let mut wtr = Writer::from_writer(File::create(OUTPUT_FILE)?);

    // Write CSV header
    wtr.write_record(&[
        "encrypted_push_id",
        "timestamp",
        "proof",
        "nullifier_hash",
        "merkle_root",
        "credential_type",
    ])?;

    for i in 1..=NUM_USERS {
        println!("Generating proof for user_{} ({}/{})", i, i, NUM_USERS);

        let public_key_str = "fb5f70676fdde3b380fc46169611672fcaf2aecd2fc33654f8750145785a3f79";
        let public_key_bytes = hex::decode(public_key_str).unwrap();
        let public_key = PublicKey::from_slice(&public_key_bytes).unwrap();

        // Generate encrypted_push_id (unique per user)
        let encrypted_push_id = public_key.seal(&mut OsRng, b"not_a_real_secret").unwrap();
        let hex_encrypted_push_id = hex::encode(encrypted_push_id);

        // Generate proof using the existing function
        let auth_request: AuthRequest =
            generate_proof(APP_ID, ACTION, &hex_encrypted_push_id, b"not_a_real_secret").await;

        // Write the record to CSV
        wtr.write_record(&[
            auth_request.encrypted_push_id,
            auth_request.timestamp.to_string(),
            auth_request.proof,
            auth_request.nullifier_hash,
            auth_request.merkle_root,
            "device".to_string(),
        ])?;

        wtr.flush()?;
    }

    println!(
        "\nSuccessfully generated test data for {} users!",
        NUM_USERS
    );
    println!("Output file: {}", OUTPUT_FILE);
    println!("\nYou can now run drill with: drill --benchmark drill.yml --stats");

    Ok(())
}
