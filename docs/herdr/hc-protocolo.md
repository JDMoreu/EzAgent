# Protocolo de herdr-core observado (hc 0.1.21, 2026-11-18)

Referencia para `ez-term`. La API completa está en `hc-schema-0.1.21.json` (OpenRPC;
`hc schema` la regenera). Esto es lo **observado en vivo** con `hc connect` en Windows.

## Transporte
- `hc connect` habla JSON-RPC 2.0 por stdin/stdout, **un mensaje JSON por línea**.
- Primero `hello`; el resto de peticiones se responden con el mismo `id`.
- **En una conexión con `events.subscribe` activo, hc no respondió más peticiones**
  (un `server.status` posterior quedó sin respuesta). Usar una conexión aparte para eventos.
- Las respuestas llegan en orden de finalización, no necesariamente de envío: emparejar por `id`.

## Intercambios reales (recortados)

```
→ {"jsonrpc":"2.0","id":1,"method":"hello","params":{"generation":1}}
← {"id":1,"jsonrpc":"2.0","result":{"core":{"id":"d5e9…","name":"desktop-ig8chfg"},"features":["framed","attach","events","status","shell","resize_notifications","automatic_input","restart_environment"],"framed":false,"generation":1,"methods":["hello","server.status",…],"version":"0.1.21",…}}

→ {"jsonrpc":"2.0","id":2,"method":"terminal.run","params":{"argv":["cmd.exe","/d","/c","echo hola-ez"],"name":"ezprobe2","labels":{"ezprobe":"1"},"no_parent":true,"env":{"EZ_PROBE":"1"}}}
← {"id":2,"jsonrpc":"2.0","result":{"address":"t72","argv":[…],"cols":200,"rows":120,"created_ms":1795049775431,"cwd":"C:\\Users\\JDMoreu","exit_code":null,"finished_ms":null,"identity":"ghostty","keep":false,"labels":{"ezprobe":"1"},"name":"ezprobe2","notify":true,"parent":null,"pid":5860,"reason":null,"state":"live","status":null,"status_ms":null,"template":null,"title":"","turn":0,…}}

→ {"jsonrpc":"2.0","id":3,"method":"terminal.read","params":{"terminal":"ezprobe2","lines":5}}
← {"id":3,"jsonrpc":"2.0","result":{"complete":true,"how":"history","text":"hola-ez"}}

→ {"jsonrpc":"2.0","id":4,"method":"terminal.bogus","params":{}}
← {"error":{"code":-32601,"data":{"hint":"see `hc schema`","kind":"unknown_method"},"message":"unknown method \"terminal.bogus\""},"id":4,"jsonrpc":"2.0"}

→ {"jsonrpc":"2.0","id":5,"method":"terminal.get","params":{"terminal":"ezprobe2"}}
← {"id":5,"jsonrpc":"2.0","result":{"address":"t72",…,"exit_code":0,"finished_ms":1795049775475,"reason":"exited","state":"finished",…}}

→ {"jsonrpc":"2.0","id":3,"method":"terminal.rm","params":{"terminal":"ezprobe3"}}
← {"id":3,"jsonrpc":"2.0","result":{"removed":"t73"}}
```

## Eventos (conexión dedicada)

```
→ {"jsonrpc":"2.0","id":2,"method":"events.subscribe","params":{"labels":{"ezprobe":"3"}}}
← {"id":2,"jsonrpc":"2.0","result":{"channel":null,"seq":156,"terminals":[]}}
← {"jsonrpc":"2.0","method":"event","params":{"data":{"address":"t73","argv":[…],"labels":{"ezprobe":"3"},"name":"ezprobe3","state":"live",…},"seq":157,"terminal":"t73","type":"created"}}
← {"jsonrpc":"2.0","method":"event","params":{"data":{"argv":["ping","-n","2","127.0.0.1"],"cwd":"C:\\Users\\JDMoreu","name":"PING.EXE","pid":10004},"seq":158,"terminal":"t73","type":"foreground"}}
← {"jsonrpc":"2.0","method":"event","params":{"data":{"exit_code":0,"finished_ms":1795049796350,"reason":"exited","signal":null},"seq":159,"terminal":"t73","type":"finished"}}
← {"jsonrpc":"2.0","method":"event","params":{"data":{},"seq":160,"terminal":"t73","type":"removed"}}
```

- `result.terminals` de la suscripción es la foto inicial (objetos `Terminal`).
- Tipos de evento según el esquema: `created`, `lost`, `restarted`, `keep`, `finished`,
  `removed`, `status`, `turn`, `title`, `labels`, `size`, `doc`; observados además
  `foreground`. También llegan notificaciones `heartbeat` cada 15 s y `missed` (seguida de
  una foto nueva) si el suscriptor se atrasa.
- `status` de un agente (objeto `Status` del esquema): `state` ∈ `idle`, `working`,
  `done`, `blocked`, `error`; `kind` (solo `blocked`) ∈ `permission`, `question`, `auth`.
