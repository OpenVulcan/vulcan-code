# Security

WebRTC DTLS/SCTP encrypts transport data. Ed25519 device keys bind Vulcan device identity to signed session metadata and DTLS fingerprints so the signaling relay cannot silently replace peers.

Device IDs are `dev_` plus lowercase base32 of the first 16 bytes of `sha256(public_key)`. Local private keys are written with `0600` permissions. TURN credentials are HMAC-SHA1 values over `user_id:session_id:expire_ts` and are rejected after expiration.
