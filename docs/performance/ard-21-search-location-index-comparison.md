# ARD-21: comparación del índice de ubicaciones en búsqueda

La búsqueda transversal carga productos, unidades y ubicaciones. Antes del cambio, cada unidad almacenada recorría la lista completa de ubicaciones para resolver código y nombre. El commit 3ccce28 construye un índice temporal por ID de ubicación y lo consulta para cada unidad. La prueba de integración de SQLite conserva la búsqueda por código P1 y verifica que el resultado de unidad incluya “P1 · Posición”.

## Carga y método

Las mediciones usan el ejemplo ard21_benchmark, con 500 unidades, 255 ubicaciones, 25 productos y 10.000 eventos en SQLite. Cada métrica toma 101 muestras después de 10 calentamientos y calcula p95 por vecino más cercano. El equipo y compilador están identificados en cada informe. Se mide el caso de uso Rust con SQLite, sin IPC, interfaz ni renderizado.

## Resultados observados

La corrida previa al cambio es una sola ejecución sobre c51aaa3. Las tres corridas posteriores son independientes sobre 3ccce28.

| Estado | Evidencia | Búsqueda p50 (ms) | Búsqueda p95 (ms) | Historial p95 (ms) | Movimiento p95 (ms) |
| --- | --- | ---: | ---: | ---: | ---: |
| Antes, una corrida | run-05 | 7,173 | 11,886 | 0,453 | 3,908 |
| Después, corrida 1 | run-06 | 5,331 | 6,534 | 0,466 | 9,441 |
| Después, corrida 2 | run-07 | 5,851 | 6,674 | 0,725 | 3,548 |
| Después, corrida 3 | run-08 | 5,914 | 7,058 | 0,512 | 3,296 |
| Después, mediana de 3 | run-06 a run-08 | 5,851 | 6,674 | 0,512 | 3,548 |

Frente a la corrida previa única, la mediana posterior observó una reducción de 18,4 % en p50 y 43,9 % en p95 de búsqueda. Las tres corridas posteriores quedaron entre 6,534 y 7,058 ms de p95. Las otras operaciones muestran variación entre corridas, especialmente el movimiento en la primera ejecución posterior.

## Interpretación y límites

El patrón es coherente con la eliminación del recorrido repetido por las ubicaciones, por lo que se conserva el índice en memoria. La comparación no demuestra por sí sola que el cambio causó toda la diferencia: solo hay una corrida previa y las métricas de CPU/SQLite fluctúan. No mezclar estos datos con la línea base ARD-21 de tres corridas ni con MR-04; para estimar el efecto con mayor confianza se requerirían tres corridas previas en la misma configuración.

Todas las búsquedas medidas cumplen la meta provisional p95 de 2.000 ms en este equipo y conjunto sintético. El resultado no representa una bodega real ni un SLA. Si el volumen excede el alcance piloto, filtrar y paginar desde SQLite seguirá siendo una decisión separada.
