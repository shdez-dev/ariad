use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{DomainError, MovementEvent, Product};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RepositoryError {
    #[error("el registro ya existe")]
    Conflict,
    #[error("el registro relacionado no existe")]
    NotFound,
    #[error("la transición solicitada no es válida para el estado actual")]
    InvalidTransition,
    #[error("error de persistencia: {0}")]
    Storage(String),
}

#[async_trait]
pub trait InventoryRepository: Send + Sync {
    async fn insert_product(&self, product: &Product) -> Result<(), RepositoryError>;

    async fn record_movement(&self, event: &MovementEvent) -> Result<(), RepositoryError>;
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl ApplicationError {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Domain(_) => "Revisa los datos ingresados y el estado actual del inventario.",
            Self::Repository(RepositoryError::Conflict) => {
                "El registro ya existe y no se guardaron cambios."
            }
            Self::Repository(RepositoryError::NotFound) => {
                "No se encontró un registro relacionado; no se guardaron cambios."
            }
            Self::Repository(RepositoryError::InvalidTransition) => {
                "El movimiento no es válido para el estado actual; no se guardaron cambios."
            }
            Self::Repository(RepositoryError::Storage(_)) => {
                "No se pudo completar la operación. No se confirmaron cambios."
            }
        }
    }
}

pub struct CreateProduct {
    repository: Arc<dyn InventoryRepository>,
}

impl CreateProduct {
    pub fn new(repository: Arc<dyn InventoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        sku: &str,
        description: &str,
        unit_of_measure: &str,
    ) -> Result<Product, ApplicationError> {
        let product = Product::new(sku, description, unit_of_measure)?;
        self.repository.insert_product(&product).await?;
        Ok(product)
    }
}

pub fn map_error_for_ui(
    diagnostic_log: &crate::diagnostic_log::DiagnosticLog,
    operation: &str,
    error: &ApplicationError,
) -> String {
    let _ = diagnostic_log.record(operation, error);
    error.user_message().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MovementEvent;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeRepository {
        saved_skus: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl InventoryRepository for FakeRepository {
        async fn insert_product(&self, product: &Product) -> Result<(), RepositoryError> {
            self.saved_skus
                .lock()
                .unwrap()
                .push(product.sku.as_str().to_owned());
            Ok(())
        }

        async fn record_movement(&self, _event: &MovementEvent) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn use_case_validates_domain_before_using_repository_port() {
        let repository = Arc::new(FakeRepository::default());
        let use_case = CreateProduct::new(repository.clone());

        let created = use_case
            .execute(" sku-7 ", " Caja", " unidad ")
            .await
            .unwrap();
        assert_eq!(created.sku.as_str(), "SKU-7");
        assert_eq!(repository.saved_skus.lock().unwrap().as_slice(), ["SKU-7"]);

        let error = use_case.execute("SKU CON ESPACIOS", "Caja", "unidad").await;
        assert!(matches!(
            error,
            Err(ApplicationError::Domain(DomainError::InvalidSku))
        ));
        assert_eq!(repository.saved_skus.lock().unwrap().len(), 1);
    }

    #[test]
    fn storage_errors_map_to_safe_user_messages() {
        let error = ApplicationError::Repository(RepositoryError::Storage(
            "detalle interno de sqlite".to_owned(),
        ));
        assert_eq!(
            error.user_message(),
            "No se pudo completar la operación. No se confirmaron cambios."
        );
        assert!(!error.user_message().contains("sqlite"));
    }

    #[test]
    fn ui_adapter_logs_internal_details_and_returns_safe_message() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("ariadna-ui-error-{unique}"));
        let log_path = directory.join("logs").join("ariadna.log");
        let log = crate::diagnostic_log::DiagnosticLog::open(&log_path).unwrap();
        let error = ApplicationError::Repository(RepositoryError::Storage(
            "sqlite busy on transaction".to_owned(),
        ));

        let message = map_error_for_ui(&log, "product.create", &error);

        assert_eq!(
            message,
            "No se pudo completar la operación. No se confirmaron cambios."
        );
        assert!(!message.contains("sqlite"));
        let log_contents = std::fs::read_to_string(&log_path).unwrap();
        assert!(log_contents.contains("product.create"));
        assert!(log_contents.contains("sqlite busy on transaction"));
        drop(log);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
