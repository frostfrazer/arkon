//! BIP39 24-word recovery mnemonic for vault migration and hardware recovery.
use arkon_core::error::{ArkonError, Result};
use bip39::Mnemonic;
use zeroize::Zeroizing;

pub fn key_to_mnemonic(key: &[u8; 32]) -> Result<Mnemonic> {
    Mnemonic::from_entropy(key)
        .map_err(|e| ArkonError::VaultError(format!("mnemonic gen: {e}")))
}

pub fn mnemonic_to_key(words: &str) -> Result<Zeroizing<[u8; 32]>> {
    let m = words.trim().parse::<Mnemonic>()
        .map_err(|e| ArkonError::VaultError(format!("invalid mnemonic: {e}")))?;
    let entropy = m.to_entropy();
    if entropy.len() != 32 {
        return Err(ArkonError::VaultError("mnemonic must be 24 words (256-bit)".into()));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&entropy);
    Ok(key)
}

pub fn display_recovery_mnemonic(mnemonic: &Mnemonic) {
    let words: Vec<&str> = mnemonic.words().collect();
    eprintln!();
    eprintln!("  \x1b[33m⚠\x1b[0m  \x1b[1mVault recovery mnemonic — write this down and store offline\x1b[0m");
    eprintln!("  \x1b[2mShown ONCE. Use with: arkon secrets recover\x1b[0m");
    eprintln!();
    for (i, chunk) in words.chunks(6).enumerate() {
        let line: Vec<String> = chunk.iter().enumerate()
            .map(|(j, w)| format!("{:2}. {:12}", i*6+j+1, w))
            .collect();
        eprintln!("  {}", line.join("  "));
    }
    eprintln!();
    eprintln!("  \x1b[31mDo NOT store this digitally without strong encryption.\x1b[0m");
    eprintln!();
}

pub fn interactive_recover() -> Result<Zeroizing<[u8; 32]>> {
    let input = dialoguer::Password::new()
        .with_prompt("Enter your 24-word recovery mnemonic (space-separated)")
        .interact()
        .map_err(|e| ArkonError::VaultError(format!("input: {e}")))?;
    mnemonic_to_key(&input)
}
