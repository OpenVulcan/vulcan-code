# Vulcan Link Protocol

Vulcan Link uses a reliable ordered WebRTC DataChannel named `vlink` and multiplexes logical streams with `VLK1` binary frames.

## Binary frame

| Field | Size | Description |
| --- | ---: | --- |
| Magic | 4 bytes | `VLK1` |
| Version | 1 byte | current value `1` |
| Type | 1 byte | OPEN/DATA/CLOSE/RESET/PING/PONG/AUTH/AUTH_OK/AUTH_FAIL |
| StreamID | 8 bytes | client-initiated streams are odd; agent-initiated streams are even |
| Length | 4 bytes | payload length |
| Payload | N bytes | JSON metadata or raw data |

`OPEN` carries JSON metadata for `http` or `websocket`. `DATA` carries raw HTTP body/response bytes or one-byte-prefixed WebSocket opcode payloads.
