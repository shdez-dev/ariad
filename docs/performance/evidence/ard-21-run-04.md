# ARD-21: medición local de capacidad y latencia

Fecha de ejecución Unix (ms): 1794435880718
Fecha UTC: 2026-11-11 22:24:40.718; hora local America/Santiago: 2026-11-11 19:24:40.718 (UTC-03:00)
Sistema: windows x86_64
Hardware: 12th Gen Intel(R) Core(TM) i5-12450H; 8 núcleos, 12 procesadores lógicos; 31,8 GiB de memoria visible
Revisión Git: 691104435059
Compilador: rustc 1.99.1 (58cf8f9c3 2026-10-13)
Perfil: release (`cargo run --release --manifest-path src-tauri/Cargo.toml --example ard21_benchmark`)
Paralelismo lógico disponible: 12
SQLite: 3.51.3
Rust crate: Ariadna 0.1.0
Base sintética: `ariadna-ard21-1794435862490688200.sqlite3`

Conjunto inicial verificado: 500 unidades (250 almacenadas y 250 despachadas), 255 ubicaciones (252 posiciones), 25 productos y 10000 eventos de movimiento aceptados. El generador usa una semilla fija implícita en los identificadores, cantidades, lotes y marcas de tiempo. Se registran 101 muestras por operación después de 10 calentamientos; p95 usa el rango de vecino más cercano. La operación de movimiento agrega 111 eventos aceptados (10 calentamientos y 101 muestras) a la base temporal después de verificar el conjunto inicial; por eso el total tras esa medición es 10111 y no se usa para describir el volumen sembrado.
La búsqueda fija `SKU-000` devuelve 21 coincidencias. El historial de `unit-000000` contiene 20 registros. El movimiento alterna `unit-000250` entre `position-000` y `position-250`.

Las duraciones miden la llamada al caso de uso Rust y su repositorio SQLite, excluyendo interfaz, serialización IPC y renderizado. El resultado corresponde solo a este equipo y configuración local; no representa un SLA ni una medición de bodega real.

| Operación | p50 (ms) | p95 (ms) | Meta p95 | Muestras |
| --- | ---: | ---: | --- | ---: |
| Búsqueda transversal de SKU-000 | 2.745 | 3.717 | Cumple | 101 |
| Historial completo de unit-000000 | 0.247 | 0.389 | Cumple | 101 |
| Movimiento SQLite de una unidad almacenada | 20.546 | 23.182 | Cumple | 101 |
