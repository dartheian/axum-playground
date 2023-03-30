# axum playground

## Bootstrap

```
cargo build
RUST_LOG=tower_http,info cargo run
```

## Features

- Resquest timeout
- Request concurrency limit
- Requests load shedding
- Graceful shutdown
- Unique ULID per request
- Tracing (with a span per request)
- JSON serialization and deserialization
- Writing to and reading from AVRO binary file

## Interaction

With httpie:

```
http localhost:3000/healthcheck
http localhost:3000/timeout
http localhost:3000/delay
http POST localhost:3000/upload content=whatever --json
http localhost:3000/download
```

## JSON input

```json
{
  "content": "whatever"
}
```

## AVRO schema

```avro
{
    "type": "record",
    "name": "test",
    "fields": [
        {"name": "content", "type": "string"}
    ]
}
```

## Example

```
❯ http POST localhost:3000/upload content=whatever --json
HTTP/1.1 200 OK
content-length: 0
date: Thu, 30 Mar 2023 15:10:07 GMT
x-request-id: 01GWSH3A3V2965YNCZ709PDD2T

❯ http localhost:3000/download
HTTP/1.1 200 OK
content-length: 21
content-type: application/json
date: Thu, 30 Mar 2023 15:10:15 GMT
x-request-id: 01GWSH3HNKSWKR2GEXBRESNVBP

[
    {
        "content": "whatever"
    }
]

❯ http localhost:3000/timeout
HTTP/1.1 408 Request Timeout
content-length: 0
date: Thu, 30 Mar 2023 15:11:40 GMT

```
