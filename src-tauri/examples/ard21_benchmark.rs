use std::{
    error::Error,
    future::Future,
    hint::black_box,
    path::PathBuf,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use ariadna_lib::{
    application::{
        CreateLocation, CreateProduct, DispatchUnit, EventOutcome, GetMovementHistory,
        InventoryRepository, MoveUnit, ReceiveLine, ReceiveUnit, SearchInventory, StoreUnit,
    },
    domain::LocationKind,
    infrastructure::sqlite::{SqliteDatabase, SqliteInventoryRepository},
};

const UNIT_COUNT: usize = 500;
const STORED_UNIT_COUNT: usize = UNIT_COUNT / 2;
const POSITION_COUNT: usize = STORED_UNIT_COUNT + 2;
const PRODUCT_COUNT: usize = 25;
const EVENTS_PER_UNIT: usize = 20;
const WARMUP_COUNT: usize = 10;
const SAMPLE_COUNT: usize = 101;
const EVENT_TIME_BASE_MS: i64 = 1_800_000_000_000;

#[derive(Debug)]
struct Measurement {
    name: &'static str,
    samples_micros: Vec<u128>,
}

impl Measurement {
    fn p50_ms(&mut self) -> f64 {
        percentile_ms(&mut self.samples_micros, 50)
    }

    fn p95_ms(&mut self) -> f64 {
        percentile_ms(&mut self.samples_micros, 95)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let database_path = database_path_from_args()?;
    if database_path.exists() {
        return Err(format!(
            "La base ya existe: {}. Usa una ruta nueva para preservar datos anteriores.",
            database_path.display()
        )
        .into());
    }

    let database = SqliteDatabase::open(&database_path).await?;
    let repository: Arc<dyn InventoryRepository> =
        Arc::new(SqliteInventoryRepository::new(&database));

    seed_dataset(repository.clone()).await?;
    verify_dataset(&database).await?;

    let search = SearchInventory::new(repository.clone());
    let history = GetMovementHistory::new(repository.clone());
    let move_unit = MoveUnit::new(repository);
    let search_result_count = search.execute("SKU-000").await?.len();
    let history_entry_count = history.execute("unit-000000").await?.len();
    if search_result_count == 0 || history_entry_count != EVENTS_PER_UNIT {
        return Err(format!(
            "Las consultas de comprobación no coinciden con la carga: búsqueda={search_result_count}, historial={history_entry_count}"
        )
        .into());
    }

    let search_measurement = measure("Búsqueda transversal de SKU-000", || {
        search.execute("SKU-000")
    })
    .await?;
    let history_measurement = measure("Historial completo de unit-000000", || {
        history.execute("unit-000000")
    })
    .await?;
    let movement_measurement = measure_moves(&move_unit).await?;

    let sqlite_version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(database.pool())
        .await?;
    let run_time_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let report_path = database_path.with_extension("md");
    let report = render_report(
        run_time_ms,
        &database_path,
        &sqlite_version,
        search_result_count,
        history_entry_count,
        [
            search_measurement,
            history_measurement,
            movement_measurement,
        ],
    );
    std::fs::write(&report_path, &report)?;

    println!("{report}");
    println!("Base sintética: {}", database_path.display());
    println!("Informe: {}", report_path.display());
    database.close().await;
    Ok(())
}

fn database_path_from_args() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = match (args.next(), args.next()) {
        (Some(path), None) => PathBuf::from(path),
        (None, None) => {
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            std::env::temp_dir().join(format!("ariadna-ard21-{timestamp}.sqlite3"))
        }
        _ => {
            return Err(
                "Uso: cargo run --release --example ard21_benchmark -- [ruta.sqlite3]".into(),
            );
        }
    };
    Ok(path)
}

async fn seed_dataset(repository: Arc<dyn InventoryRepository>) -> Result<(), Box<dyn Error>> {
    let create_product = CreateProduct::new(repository.clone());
    for product_index in 0..PRODUCT_COUNT {
        create_product
            .execute(
                &format!("SKU-{product_index:03}"),
                &format!("Producto sintético {product_index:03}"),
                "unidad",
            )
            .await?;
    }

    let create_location = CreateLocation::new(repository.clone());
    create_location
        .execute("zone-01", "Z01", "Zona sintética", LocationKind::Zone, None)
        .await?;
    create_location
        .execute(
            "aisle-01",
            "A01",
            "Pasillo sintético",
            LocationKind::Aisle,
            Some("zone-01"),
        )
        .await?;
    create_location
        .execute(
            "rack-01",
            "R01",
            "Rack sintético",
            LocationKind::Rack,
            Some("aisle-01"),
        )
        .await?;
    for position_index in 0..POSITION_COUNT {
        create_location
            .execute(
                &format!("position-{position_index:03}"),
                &format!("P{position_index:03}"),
                &format!("Posición sintética {position_index:03}"),
                LocationKind::Position,
                Some("rack-01"),
            )
            .await?;
    }

    let receive_unit = ReceiveUnit::new(repository.clone());
    let store_unit = StoreUnit::new(repository.clone());
    let move_unit = MoveUnit::new(repository.clone());
    let dispatch_unit = DispatchUnit::new(repository);
    let mut occurred_at_ms = EVENT_TIME_BASE_MS;

    for unit_index in 0..UNIT_COUNT {
        let unit_id = format!("unit-{unit_index:06}");
        let unit_label = format!("PALLET-{unit_index:06}");
        let product_index = unit_index % PRODUCT_COUNT;
        receive_unit
            .execute(
                &unit_id,
                &unit_label,
                vec![ReceiveLine {
                    sku: format!("SKU-{product_index:03}"),
                    quantity: 1 + (unit_index % 40) as u32,
                    lot_code: Some(format!("LOT-{unit_index:06}")),
                }],
                &event_id(unit_index, 0),
                next_event_time(&mut occurred_at_ms),
                "ard21-seed",
            )
            .await?;

        let assigned_position_index = unit_index % STORED_UNIT_COUNT;
        let assigned_position = format!("position-{assigned_position_index:03}");
        store_unit
            .execute(
                &event_id(unit_index, 1),
                &unit_id,
                &assigned_position,
                next_event_time(&mut occurred_at_ms),
                "ard21-seed",
            )
            .await?;

        let dispatched = unit_index < STORED_UNIT_COUNT;
        let move_count = if dispatched {
            EVENTS_PER_UNIT - 3
        } else {
            EVENTS_PER_UNIT - 2
        };
        let reserve_position = "position-250";
        let mut current_position = assigned_position.clone();
        for event_index in 0..move_count {
            let destination = if current_position == assigned_position {
                reserve_position
            } else {
                assigned_position.as_str()
            };
            move_unit
                .execute(
                    &event_id(unit_index, event_index + 2),
                    &unit_id,
                    &current_position,
                    destination,
                    next_event_time(&mut occurred_at_ms),
                    "ard21-seed",
                )
                .await?;
            current_position = destination.to_owned();
        }

        if dispatched {
            dispatch_unit
                .execute(
                    &event_id(unit_index, EVENTS_PER_UNIT - 1),
                    &unit_id,
                    &current_position,
                    next_event_time(&mut occurred_at_ms),
                    "ard21-seed",
                )
                .await?;
        }
    }
    Ok(())
}

fn event_id(unit_index: usize, event_index: usize) -> String {
    format!("seed-u{unit_index:03}-e{event_index:02}")
}

fn next_event_time(occurred_at_ms: &mut i64) -> i64 {
    let timestamp = *occurred_at_ms;
    *occurred_at_ms += 1;
    timestamp
}

async fn verify_dataset(database: &SqliteDatabase) -> Result<(), Box<dyn Error>> {
    let unit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM handling_units")
        .fetch_one(database.pool())
        .await?;
    let location_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM locations")
        .fetch_one(database.pool())
        .await?;
    let position_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM locations WHERE kind = 'position'")
            .fetch_one(database.pool())
            .await?;
    let movement_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movement_events")
        .fetch_one(database.pool())
        .await?;
    let stored_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM handling_units WHERE status = 'stored'")
            .fetch_one(database.pool())
            .await?;
    let dispatched_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM handling_units WHERE status = 'dispatched'")
            .fetch_one(database.pool())
            .await?;
    let rejected_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM movement_events WHERE result <> 'accepted'")
            .fetch_one(database.pool())
            .await?;

    if unit_count != UNIT_COUNT as i64
        || location_count < 250
        || position_count < 250
        || movement_count != (UNIT_COUNT * EVENTS_PER_UNIT) as i64
        || stored_count != STORED_UNIT_COUNT as i64
        || dispatched_count != (UNIT_COUNT - STORED_UNIT_COUNT) as i64
        || rejected_count != 0
    {
        return Err(format!(
            "El conjunto generado no cumple los invariantes: unidades={unit_count}, ubicaciones={location_count}, posiciones={position_count}, almacenadas={stored_count}, despachadas={dispatched_count}, eventos={movement_count}, rechazados={rejected_count}"
        )
        .into());
    }
    Ok(())
}

async fn measure<F, Fut, T, E>(name: &'static str, mut operation: F) -> Result<Measurement, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut samples_micros = Vec::with_capacity(SAMPLE_COUNT);
    for sample_index in 0..(WARMUP_COUNT + SAMPLE_COUNT) {
        let started = Instant::now();
        let outcome = operation().await?;
        let elapsed = started.elapsed();
        black_box(outcome);
        if sample_index >= WARMUP_COUNT {
            samples_micros.push(elapsed.as_micros());
        }
    }
    Ok(Measurement {
        name,
        samples_micros,
    })
}

async fn measure_moves(move_unit: &MoveUnit) -> Result<Measurement, Box<dyn Error>> {
    let mut samples_micros = Vec::with_capacity(SAMPLE_COUNT);
    let mut current_position = "position-000";
    for sample_index in 0..(WARMUP_COUNT + SAMPLE_COUNT) {
        let destination = if current_position == "position-000" {
            "position-250"
        } else {
            "position-000"
        };
        let event_id = format!("measure-move-{sample_index:03}");
        let started = Instant::now();
        let outcome = move_unit
            .execute(
                &event_id,
                "unit-000250",
                current_position,
                destination,
                EVENT_TIME_BASE_MS + 20_000 + sample_index as i64,
                "ard21-measurement",
            )
            .await?;
        let elapsed = started.elapsed();
        if outcome != EventOutcome::Applied {
            return Err(format!("El movimiento de medición no se aplicó: {outcome:?}").into());
        }
        current_position = destination;
        if sample_index >= WARMUP_COUNT {
            samples_micros.push(elapsed.as_micros());
        }
    }
    Ok(Measurement {
        name: "Movimiento SQLite de una unidad almacenada",
        samples_micros,
    })
}

fn percentile_ms(samples_micros: &mut [u128], percentile: usize) -> f64 {
    samples_micros.sort_unstable();
    let rank = (samples_micros.len() * percentile).div_ceil(100);
    samples_micros[rank.saturating_sub(1)] as f64 / 1_000.0
}

fn render_report(
    run_time_ms: u128,
    database_path: &std::path::Path,
    sqlite_version: &str,
    search_result_count: usize,
    history_entry_count: usize,
    mut measurements: [Measurement; 3],
) -> String {
    let mut report = format!(
        "# ARD-21: medición local de capacidad y latencia\n\n\
         Fecha de ejecución Unix (ms): {run_time_ms}\n\
         Sistema: {} {}\n\
         Paralelismo lógico disponible: {}\n\
         SQLite: {sqlite_version}\n\
         Rust crate: Ariadna {}\n\
         Ruta de la base sintética: `{}`\n\n\
         Conjunto: {UNIT_COUNT} unidades ({} almacenadas y {} despachadas), {} ubicaciones ({} posiciones), {PRODUCT_COUNT} productos y {} eventos de movimiento aceptados. El generador usa una semilla fija implícita en los identificadores, cantidades, lotes y marcas de tiempo. Se registran {} muestras por operación después de {} calentamientos; p95 usa el rango de vecino más cercano.\n\
         La búsqueda fija `SKU-000` devuelve {search_result_count} coincidencias. El historial de `unit-000000` contiene {history_entry_count} registros. El movimiento alterna `unit-000250` entre `position-000` y `position-250`.\n\n\
         Las duraciones miden la llamada al caso de uso Rust y su repositorio SQLite, excluyendo interfaz, serialización IPC y renderizado. El resultado corresponde solo a este equipo y configuración local; no representa un SLA ni una medición de bodega real.\n\n\
         | Operación | p50 (ms) | p95 (ms) | Muestras |\n\
         | --- | ---: | ---: | ---: |\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(1, usize::from),
        env!("CARGO_PKG_VERSION"),
        database_path.display(),
        STORED_UNIT_COUNT,
        UNIT_COUNT - STORED_UNIT_COUNT,
        POSITION_COUNT + 3,
        POSITION_COUNT,
        UNIT_COUNT * EVENTS_PER_UNIT,
        SAMPLE_COUNT,
        WARMUP_COUNT,
    );

    for measurement in &mut measurements {
        let p50 = measurement.p50_ms();
        let p95 = measurement.p95_ms();
        report.push_str(&format!(
            "| {} | {p50:.3} | {p95:.3} | {} |\n",
            measurement.name,
            measurement.samples_micros.len()
        ));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::percentile_ms;

    #[test]
    fn percentile_uses_nearest_rank() {
        let mut samples = (1_u32..=100)
            .map(|sample| u128::from(sample) * 1_000)
            .collect::<Vec<_>>();

        assert_eq!(percentile_ms(&mut samples, 50), 50.0);
        assert_eq!(percentile_ms(&mut samples, 95), 95.0);
    }
}
