# Relay

The relay exposes:

- `/v1/signal` for WebSocket signaling.
- `/v1/turn-credentials` for short-lived HMAC TURN credentials.
- `/healthz` for readiness checks.

It stores online devices in memory, forwards `connect_request`, `sdp_offer`, `sdp_answer`, and `ice_candidate` messages to the target device, and does not inspect tunneled HTTP/WebSocket payloads.
