use {
    crate::domain::error::PipelineError,
    crate::domain::id::{EventId, ExternalId},
    crate::domain::payment::WebhookTrigger,
    crate::domain::provider::PaymentProvider,
    crate::infra::postgres::job_repo,
    crate::services::payment_pipeline::process_webhook,
    sqlx::PgPool,
    std::sync::Arc,
    tokio::sync::watch,
};

/// Poll for pending jobs and process them via the existing payment pipeline.
pub async fn run_worker(
    pool: PgPool,
    provider: Arc<dyn PaymentProvider>,
    mut shutdown: watch::Receiver<bool>,
) {
    tracing::info!("job worker started");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                tracing::info!("job worker shutting down");
                return;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
        }

        if let Err(e) = poll_once(&pool, &*provider).await {
            tracing::error!(error = %e, "worker poll error");
        }
    }
}

async fn poll_once(pool: &PgPool, provider: &dyn PaymentProvider) -> Result<(), PipelineError> {
    let mut tx = pool.begin().await?;
    let jobs = job_repo::claim(&mut tx, 10).await?;
    tx.commit().await?;

    for job in jobs {
        let event_id = match EventId::new(&job.event_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(event_id = %job.event_id, error = %e, "invalid event_id, completing as garbage");
                job_repo::complete(pool, job.id).await?;
                continue;
            }
        };

        let external_id = match ExternalId::new(&job.object_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(object_id = %job.object_id, error = %e, "invalid external_id, completing as garbage");
                job_repo::complete(pool, job.id).await?;
                continue;
            }
        };

        let trigger = WebhookTrigger::Payment {
            event_id,
            event_type: job.event_type,
            external_id,
            raw_event: job.raw_event,
            provider_ts: job.provider_ts,
        };

        match process_webhook(pool, provider, trigger, "worker:stripe").await {
            Ok(result) => {
                tracing::info!(job_id = %job.id, ?result, "job processed");
                job_repo::complete(pool, job.id).await?;
            }
            Err(PipelineError::Validation(msg)) => {
                tracing::warn!(job_id = %job.id, error = %msg, "validation error, completing (no retry)");
                job_repo::complete(pool, job.id).await?;
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "job failed, scheduling retry");
                job_repo::fail(pool, job.id, &e.to_string()).await?;
            }
        }
    }

    Ok(())
}

/// Periodically reset jobs stuck in 'processing' back to 'pending'.
pub async fn run_reaper(pool: PgPool, mut shutdown: watch::Receiver<bool>) {
    tracing::info!("stale job reaper started");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                tracing::info!("stale job reaper shutting down");
                return;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
        }

        match job_repo::reap_stale(&pool).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(count = n, "reaped stale jobs"),
            Err(e) => tracing::error!(error = %e, "reaper error"),
        }
    }
}
