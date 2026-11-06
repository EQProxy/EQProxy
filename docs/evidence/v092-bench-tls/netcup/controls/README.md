# TLS sink controls — netcup, 2026-11-03

These are **calibration controls**, not proxy results. They answer one narrow
question left by the S-TLS-02 collapse: can the same `oha 1.16.0` client sustain
the 20-connection / full-handshake shape for five consecutive 5 s runs against
a direct TLS sink?

- Host: `netcup-bench`, `x86_64`, Linux 6.8.0-147, 8 vCPU exposed.
- Generator: `/root/bin/oha-1.16.0`.
- Sink: static Go `net/http` TLS 1.3 / HTTP/1.1 server, `/1k`,
  `sha256:2d9506fa302b9fac84d22cadd60172636fe66585cf105c9fe8923f5a5b8e5bb1`.
- Certificate: the same fixture as the campaign, SHA-256
  `0c6cab2ee350fe29fa65783b19baa55c19444de71e9926e35ef093be4d018017`.
- Client flags: `-z 5s -c 20 --disable-keepalive --http-version 1.1 --cacert
  cert.pem`, no `--insecure`. Every run returned only HTTP 200 and zero errors.
- Raw files: `sink-run-{1..5}.json` and `docker-sink-run-{1..5}.json`;
  SHA-256 values in `checksums.sha256`.

## 1. Sink as a host process

The sink listened directly on host port 19603, outside Docker:

```bash
/root/bench-v092-tls/sink-amd64 -addr :19603 \
  -cert /root/bench-v092-tls/s-tls-01/certs/cert.pem \
  -key  /root/bench-v092-tls/s-tls-01/certs/key.pem
```

| Run | RPS | p95 ms | Responses |
|---:|---:|---:|---|
| 1 | 8,195.5 | 5.563 | 41,014 × 200 |
| 2 | 8,391.4 | 5.430 | 42,005 × 200 |
| 3 | 8,629.2 | 5.312 | 43,182 × 200 |
| 4 | 7,931.4 | 5.716 | 39,706 × 200 |
| 5 | 7,871.6 | 5.837 | 39,388 × 200 |

No run collapsed. After the five runs, `ss -s` reported **2** TCP sockets in
`TIME_WAIT`.

## 2. Sink behind a Docker published port

The same binary ran in `python:3.14-slim`, with the same bridge shape the proxy
campaign uses:

```bash
docker network create benchctl
docker run -d --name tls-sink-control --network benchctl \
  --ulimit nofile=65536:65536 \
  -p 127.0.0.1:19603:19003 \
  -v /root/bench-v092-tls/sink-amd64:/sink:ro \
  -v /root/bench-v092-tls/s-tls-01/certs:/certs:ro \
  python:3.14-slim /sink -addr :19003 \
  -cert /certs/cert.pem -key /certs/key.pem
```

| Run | RPS | p95 ms | Responses |
|---:|---:|---:|---|
| 1 | 5,924.5 | 6.531 | 29,649 × 200 |
| 2 | 5,793.8 | 6.684 | 28,997 × 200 |
| 3 | 5,934.3 | 6.530 | 29,705 × 200 |
| 4 | 5,709.7 | 6.806 | 28,576 × 200 |
| 5 | 5,731.8 | 6.831 | 28,685 × 200 |

No run collapsed either. The published-port path is visibly slower than the
host process, which is why the corrected S-TLS-02 campaign runs the sink on the
host network; it is still not a generic generator ceiling. After the five runs,
`ss -s` reported **12,343** TCP sockets in `TIME_WAIT`.

## 3. What this does and does not establish

- It establishes that the **same client and sink** can complete five 5 s
  full-handshake runs at 5.7–8.6k RPS without the 576–319–306 RPS collapse.
- It does **not** identify the cause of the EQProxy collapse. The proxy runs
  through a different container path and does its own TLS termination; the sink
  controls do not exercise that code.
- It is not a substitute for a CPU profile. `perf` remains **NOT EXECUTED**
  (`kernel.perf_event_paranoid=4`), so the proxy-side cause is still unproven.
- Host load1 is not sampled per control run; these controls are diagnostic, not
  a new benchmark series.
