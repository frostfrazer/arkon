# ARKON Security Model

## Vault key hierarchy — dual-factor encryption

The machine fingerprint is a **salt**, not the sole secret.
An attacker who steals the vault file AND reads the world-readable machine-id
still cannot decrypt without the passphrase.

```
user_passphrase         (blind factor)
      +
machine_fingerprint     (Linux: /etc/machine-id  macOS: IOPlatformUUID  Windows: MachineGuid)
      |
      v
combined_salt = SHA-256(machine_fingerprint || random_vault_salt)
Argon2id(passphrase, combined_salt, m=64MB, t=3, p=4) -> 256-bit vault_key
AES-256-GCM(vault_key, OsRng 96-bit nonce) -> ciphertext
```

### OS keychain session cache
Prompts once per session, then caches in OS keychain.
Run `arkon secrets lock` to clear immediately.

### CI / headless
`ARKON_VAULT_KEY=<64-char hex>` bypasses passphrase entirely.

### Recovery mnemonic
24-word BIP39 mnemonic displayed once on vault creation.
Store offline. `arkon secrets recover` re-encrypts on new hardware.

### File permissions
```
~/.arkon/               0700
~/.arkon/vault/         0700
*.vault                 0600  (atomic write)
*.vault.bak             0600
~/.arkon/identity/*.key 0600
```

## ACME / Let's Encrypt account key

Stored encrypted in vault under `__acme__<domain>`.
Plaintext fallback (0600) only when vault unavailable, with warning.

## P2P peer identity

Ed25519 seed stored encrypted in vault under `__peer_identity__`.
Plaintext fallback (0600) only when vault unavailable, with warning.

## SSH host key verification

Default: `StrictHostKeyChecking=yes` - refuses unknown hosts.

```toml
[targets.production]
accept_new_host = true  # accept-new but still rejects changed keys
```

Add new hosts first:
```bash
ssh-keyscan myserver.com >> ~/.ssh/known_hosts
```

## Relay architecture

The relay is an HTTP proxy - operator can read preview traffic.
For sensitive content: use private relay (relay/main.go) or SSH target.
DHT direct connections (no relay) use Noise protocol encryption.

## Threat model

| Threat | Status | Mitigation |
|--------|--------|-----------|
| Vault stolen + machine-id known | OK | Passphrase still required |
| Hardware migration data loss | OK | 24-word BIP39 mnemonic |
| ACME key stolen | OK | Stored in encrypted vault |
| P2P identity stolen | OK | Stored in encrypted vault |
| SSH MITM first connection | OK | StrictHostKeyChecking=yes default |
| SSH host key substitution | OK | Strict mode rejects changed keys |
| Relay reads preview | PARTIAL | Relay sees HTTP; use private relay for sensitive content |
| Audit log tampered | OK | HMAC chain breaks on modification |
| Vault brute-force | OK | Argon2id m=64MB |
