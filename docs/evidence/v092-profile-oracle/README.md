# Oracle arm64: intentos de perfil (NOT EXECUTED)

Fecha: 2026-11-04. Host `oracle-quasar` (linux/arm64, 4 CPU).

Estos intentos **no producen un perfil**: no hay `PPROF_SVG_BEGIN/END` y no se
usan para atribuir CPU ni funciones. Se conservan para que la limitación sea
reproducible.

## H2-01

- Imagen: `eqproxy-profile:v092-fixed`
  (`sha256:23c429d2bdf3b68b…`), reconstruida desde el árbol actual con
  `--target profile --build-arg PROFILE=1`.
- Comando: `python3 scripts/bench_v092_h2.py --reps 1 --scenarios S-H2-01
  --eq-image eqproxy-profile:v092-fixed --profile ...`.
- Resultado observado: el contenedor `h2bench-eqproxy` salió con **139
  (SIGSEGV)** durante la carga; no hay marcadores SVG. Log:
  `h2-01/run.log`.
- Control con la imagen release: `h2-release/` completa S-H2-01 con
  18 669 vs 25 463 rps, 0 errores. El escenario es válido; el fallo es de la
  imagen de perfil.

## H3-01

- Misma imagen y host.
- Comando: `python3 scripts/evidence/v092-h3-bench/run.py --reps 1
  --eq-image eqproxy-profile:v092-fixed --profile ...`.
- Resultado: calibración de carga del sink 27 400,9 rps; calibración de
  handshake 0; al entrar en S-H3-01 el contenedor `h3bench-eqproxy` salió con
  **139 (SIGSEGV)**. Sink y origen siguieron vivos; sin SVG. Log:
  `h3-01/run.log`.

## Decisión

Los perfiles de escenario se tomaron en netcup (`v092-profile-summary.md`);
los perfiles arm64 de Oracle quedan **NOT EXECUTED** y no se presentan como
PASS.
