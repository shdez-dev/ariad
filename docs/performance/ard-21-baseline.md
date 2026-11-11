# ARD-21: línea base de capacidad y latencia

Estado: medido localmente el 11 de noviembre de 2026. La carga es sintética y reproducible; estos resultados describen el equipo indicado y no representan una garantía de producción ni una operación real de bodega.

## Escenario

El ejemplo `src-tauri/examples/ard21_benchmark.rs` crea una base SQLite temporal nueva y usa los casos de uso de Ariadna para sembrar los datos. No modifica el perfil de usuario ni escribe directamente filas de inventario. Cada repetición obtiene otra base con los mismos identificadores, relaciones, cantidades, lotes y marcas de tiempo.

| Dato | Volumen y distribución |
| --- | --- |
| Productos | 25 productos activos |
| Unidades de manipulación | 500: 250 almacenadas y 250 despachadas, cada una con una línea |
| Ubicaciones | 255 en total: una zona, un pasillo, un rack y 252 posiciones |
| Eventos | 10.000 movimientos aceptados, 20 por unidad; ningún movimiento rechazado |
| Posiciones libres | Dos; una sirve de destino alterno para el movimiento medido |

La secuencia de siembra respeta la jerarquía, unicidad de posiciones, reglas de estado e historial append-only de la aplicación. Antes de medir, el ejemplo comprueba las cantidades exactas de productos, líneas, unidades, ubicaciones, estados y eventos, confirma que `movement_attempts` esté vacío y verifica las 21 coincidencias de búsqueda y las 20 entradas del historial.

## Operaciones medidas

Cada operación se mide en el caso de uso Rust y el repositorio SQLite real, incluyendo la lógica de aplicación, consultas y transacciones. No se mide el tiempo de interfaz, serialización IPC ni renderizado.

| Operación | Carga medida |
| --- | --- |
| Búsqueda transversal | Ejecutar `SearchInventory` con `SKU-000`; obtiene 21 coincidencias en productos y unidades |
| Historial | Ejecutar `GetMovementHistory` para `unit-000000`; obtiene 20 registros |
| Movimiento | Medir 101 ejecuciones de `MoveUnit` sobre `unit-000250`, alternando entre `position-000` y la posición libre `position-250`, más 10 calentamientos |

Para cada operación se descartan 10 calentamientos y se guardan 101 muestras. El p95 usa el rango de vecino más cercano. Las tres operaciones se ejecutan en secuencia sobre una base aislada por repetición. La compilación es `release`, con el perfil optimizado de Ariadna; la base se abre con la configuración de la aplicación: SQLite, claves foráneas activas, WAL, cinco conexiones y espera de bloqueo de cinco segundos.

El movimiento añade 111 eventos de medición (10 calentamientos y 101 muestras) a la copia temporal después de verificar la base inicial de 10.000 eventos. Las consultas de búsqueda e historial se miden primero contra la base inicial. Cada repetición crea una base nueva, así que las escrituras de una corrida no se acumulan en las siguientes.

## Equipo y versiones

| Propiedad | Valor |
| --- | --- |
| Sistema operativo | Windows 11 Pro, build 26200, x64 |
| Procesador | Intel Core i5-12450H, 8 núcleos y 12 procesadores lógicos |
| Memoria visible | 31,8 GiB |
| Rust | rustc 1.99.1 (58cf8f9c3, 13-10-2026) |
| SQLite | 3.51.3 |
| Aplicación | Ariadna 0.1.0 |
| Ubicación de las bases | Carpeta temporal local de Windows |
| Revisión medida | `3acb83948047` |

## Resultados

Se hicieron tres ejecuciones independientes con la misma configuración, la misma carga y el mismo commit. La tabla principal muestra la mediana de los tres p50 y la mediana de los tres p95. La columna de rango expone la variación observada entre ejecuciones. Los informes originales incluyen commit, compilador y hardware capturados por el programa: [repetición 1](evidence/ard-21-run-01.md), [repetición 2](evidence/ard-21-run-02.md) y [repetición 3](evidence/ard-21-run-03.md).

| Operación | Mediana p50 (ms) | Mediana p95 (ms) | Rango p95 (ms) | Meta p95 |
| --- | ---: | ---: | ---: | --- |
| Búsqueda transversal | 6,744 | 10,057 | 8,442 a 25,883 | Cumple, <= 2.000 ms |
| Historial por unidad | 0,345 | 0,561 | 0,469 a 0,565 | Cumple, <= 2.000 ms |
| Movimiento SQLite | 2,519 | 7,255 | 3,346 a 191,665 | Cumple, <= 2.000 ms |

Resultados p50/p95 de cada repetición:

| Repetición | Búsqueda (ms) | Historial (ms) | Movimiento (ms) |
| --- | --- | --- | --- |
| [1](evidence/ard-21-run-01.md) | 11,258 / 25,883 | 0,292 / 0,561 | 2,519 / 7,255 |
| [2](evidence/ard-21-run-02.md) | 6,744 / 8,442 | 0,345 / 0,469 | 59,470 / 191,665 |
| [3](evidence/ard-21-run-03.md) | 6,005 / 10,057 | 0,389 / 0,565 | 2,002 / 3,346 |

La meta provisional de p95 <= 2 s se cumple en las tres repeticiones. El mayor p95 observado fue 191,665 ms en movimientos. Las mediciones muestran dispersión, pero no aíslan su causa; los rangos y los tres informes se conservan para que el resultado no se reduzca a una sola corrida.

### Repetición complementaria del 11 de noviembre de 2026

Se hizo una ejecución adicional sobre la revisión `691104435059`, con la misma carga, equipo y compilador. El informe completo está en [ard-21-run-04.md](evidence/ard-21-run-04.md). Esta corrida se registra separada del conjunto inicial de tres: sus valores no se combinan con las medianas ni los rangos anteriores.

| Operación | p50 (ms) | p95 (ms) | Muestras | Meta p95 |
| --- | ---: | ---: | ---: | --- |
| Búsqueda transversal de SKU-000 | 2,745 | 3,717 | 101 | Cumple |
| Historial completo de unit-000000 | 0,247 | 0,389 | 101 | Cumple |
| Movimiento SQLite de una unidad almacenada | 20,546 | 23,182 | 101 | Cumple |

La ejecución se compiló con perfil `release` mediante el comando reproducible indicado en este documento. Cada operación tuvo 10 calentamientos previos. La medición de movimiento añade 111 eventos aceptados a la base temporal luego de verificar la carga inicial de 10.000; no cambia el conteo que describe el conjunto sembrado. La medición cubre el caso de uso Rust y SQLite; excluye ventana, serialización IPC, renderizado y concurrencia. Es una repetición en un solo equipo, no una nueva línea base estadística ni una medición de bodega. Los tres p95 observados están por debajo de la meta provisional de 2.000 ms.

## Reproducir

Desde la raíz del repositorio, con Rust y las herramientas de compilación de Windows instaladas:

```powershell
cargo run --release --manifest-path src-tauri/Cargo.toml --example ard21_benchmark
```

El comando crea una base sintética y un informe Markdown con el mismo nombre bajo la carpeta temporal del sistema. También se puede indicar una ruta nueva como argumento. El programa rechaza rutas o informes preexistentes y nunca los sobrescribe. El informe registra el commit, `rustc`, versión de SQLite, sistema y hardware detectados, además de indicar por operación si cumple la meta; guarda solo el nombre de la base para no incluir una ruta local de usuario. Las evidencias archivadas de las tres ejecuciones que sustentan las tablas se conservan en `docs/performance/evidence/`.

## Limitaciones y lectura

1. La búsqueda ejecuta el caso de uso completo, que lista los catálogos y filtra en memoria; el escenario mide su comportamiento actual con el volumen especificado.
2. Cada unidad recibe exactamente 20 eventos y se alternan dos estados finales. Esta distribución facilita reproducibilidad, pero no proviene de estadísticas de una bodega.
3. Los tiempos no incluyen la ventana Tauri ni el transporte IPC. La tarjeta no valida percepción de velocidad ni interacción visual.
4. La prueba es local, con una sola instancia y una sola operación medida a la vez. No mide concurrencia, importaciones, varias bodegas ni crecimiento por encima de 500 unidades.
5. El p95 de 2 s sigue siendo un objetivo provisional del prototipo; estas mediciones no sustituyen un piloto con datos y equipo de una bodega.
