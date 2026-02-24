use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use diesel::prelude::*;
use diesel::r2d2::{self, ConnectionManager, Pool};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::info;
use crate::models::schema;
use crate::models::models::{ChainId, IntentStatus, IntentStore, ShadowIntent};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub type DbPool = Pool<ConnectionManager<PgConnection>>;

#[derive(Clone)]
pub struct Database {
    pub pool: DbPool,
}

impl Database {
    pub fn new(database_url: &str, max_connections: u32) -> Result<Self> {
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        let pool = Pool::builder()
            .max_size(max_connections)
            .build(manager)
            .context("Failed to create database pool")?;

        Ok(Database { pool })
    }

    pub fn from_env() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").context("DATABASE_URL environment variable not set")?;

        let max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);

        Self::new(&database_url, max_connections)
    }

    pub fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations...");
        let mut conn = self.get_connection()?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("Migration error: {}", e))?;
        info!("Migrations completed");
        Ok(())
    }

    pub fn get_connection(
        &self,
    ) -> Result<r2d2::PooledConnection<ConnectionManager<PgConnection>>> {
        self.pool
            .get()
            .context("Failed to get database connection")
    }

    pub fn health_check(&self) -> Result<()> {
        let mut conn = self.get_connection()?;
        diesel::sql_query("SELECT 1")
            .execute(&mut conn)
            .context("Database health check failed")?;
        Ok(())
    }

    // ==========================================================
    //  COMMITMENT EVENT LOG
    //  Event listener captures on-chain CommitmentAdded events
    //  here with block ordering for reorg recovery, then feeds
    //  them into merkle_manager which writes to merkle_leaves.
    // ==========================================================

    pub fn get_commitments_by_chain(&self, chain_name: &str) -> Result<Vec<String>> {
        use schema::commitments::dsl::*;

        let mut conn = self.get_connection()?;

        commitments
            .filter(chain.eq(chain_name))
            .order((block_number.asc(), log_index.asc()))
            .select(commitment_hash)
            .load::<String>(&mut conn)
            .context("Failed to get commitments")
    }

    /// Returns true if any intent already uses this nullifier_hash.
    /// Used at intake to reject duplicate nullifiers before they reach the DB.
    pub fn nullifier_hash_exists(&self, nullifier: &str) -> Result<bool> {
        use schema::shadow_intents::dsl::*;
        let mut conn = self.get_connection()?;
        let count: i64 = shadow_intents
            .filter(nullifier_hash.eq(nullifier))
            .count()
            .get_result(&mut conn)
            .context("Failed to check nullifier_hash uniqueness")?;
        Ok(count > 0)
    }

    pub fn add_commitment_record(
        &self,
        chain_name: &str,
        hash: &str,
        intent_id_val: Option<&str>,
        block_num: Option<u64>,
        log_idx: Option<u32>,
    ) -> Result<()> {
        use schema::commitments;

        let mut conn = self.get_connection()?;

        diesel::insert_into(commitments::table)
            .values((
                commitments::chain.eq(chain_name),
                commitments::commitment_hash.eq(hash),
                commitments::intent_id.eq(intent_id_val),
                commitments::block_number.eq(block_num.map(|b| b as i64)),
                commitments::log_index.eq(log_idx.map(|i| i as i32)),
                commitments::created_at.eq(Utc::now()),
            ))
            .on_conflict(commitments::commitment_hash)
            .do_nothing()
            .execute(&mut conn)
            .context("Failed to add commitment")?;

        Ok(())
    }
}

// ==========================================================
//  IntentStore TRAIT IMPLEMENTATION
//  Tables: shadow_intents, merkle_leaves, merkle_roots,
//          transaction_logs
//  Consumers: relay_coordinator, merkle_manager, root_sync
// ==========================================================

impl IntentStore for Database {
    fn save_intent(&self, intent: &ShadowIntent) -> Result<()> {
        use schema::shadow_intents;

        let mut conn = self.get_connection()?;

        diesel::insert_into(shadow_intents::table)
            .values((
                shadow_intents::id.eq(&intent.id),
                shadow_intents::commitment.eq(&intent.commitment),
                shadow_intents::nullifier_hash.eq(&intent.nullifier_hash),
                shadow_intents::view_key.eq(&intent.view_key),
                shadow_intents::near_intents_id.eq(&intent.near_intents_id),
                shadow_intents::source_chain.eq(intent.source_chain.as_chain_id_str()),
                shadow_intents::dest_chain.eq(intent.dest_chain.as_chain_id_str()),
                shadow_intents::encrypted_recipient.eq(&intent.encrypted_recipient),
                shadow_intents::token.eq(&intent.token),
                shadow_intents::amount.eq(&intent.amount),
                shadow_intents::status.eq(intent.status.as_str()),
                shadow_intents::deposit_address.eq(&intent.deposit_address),
                shadow_intents::near_correlation_id.eq(&intent.near_correlation_id),
                shadow_intents::near_status.eq(&intent.near_status),
                shadow_intents::dest_tx_hash.eq(&intent.dest_tx_hash),
                shadow_intents::settle_tx_hash.eq(&intent.settle_tx_hash),
                shadow_intents::source_settle_tx_hash.eq(&intent.source_settle_tx_hash),
                shadow_intents::encrypted_secret.eq(&intent.encrypted_secret),
                shadow_intents::encrypted_nullifier.eq(&intent.encrypted_nullifier),
                shadow_intents::created_at.eq(intent.created_at as i64),
                shadow_intents::updated_at.eq(intent.updated_at as i64),
            ))
            .on_conflict(shadow_intents::id)
            .do_update()
            .set((
                shadow_intents::status.eq(intent.status.as_str()),
                shadow_intents::deposit_address.eq(&intent.deposit_address),
                shadow_intents::near_correlation_id.eq(&intent.near_correlation_id),
                shadow_intents::near_status.eq(&intent.near_status),
                shadow_intents::dest_tx_hash.eq(&intent.dest_tx_hash),
                shadow_intents::settle_tx_hash.eq(&intent.settle_tx_hash),
                shadow_intents::source_settle_tx_hash.eq(&intent.source_settle_tx_hash),
                shadow_intents::encrypted_secret.eq(&intent.encrypted_secret),
                shadow_intents::encrypted_nullifier.eq(&intent.encrypted_nullifier),
                shadow_intents::updated_at.eq(Utc::now().timestamp()),
            ))
            .execute(&mut conn)
            .context("Failed to save shadow intent")?;

        Ok(())
    }

    fn get_intent(&self, intent_id: &str) -> Result<Option<ShadowIntent>> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        let result = shadow_intents
            .filter(id.eq(intent_id))
            .first::<DbShadowIntent>(&mut conn)
            .optional()
            .context("Failed to get shadow intent")?;

        Ok(result.map(ShadowIntent::from))
    }

    fn get_intents_by_status(&self, status_filter: IntentStatus) -> Result<Vec<ShadowIntent>> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        let results = shadow_intents
            .filter(status.eq(status_filter.as_str()))
            .order(created_at.asc())
            .load::<DbShadowIntent>(&mut conn)
            .context("Failed to get shadow intents by status")?;

        Ok(results.into_iter().map(ShadowIntent::from).collect())
    }

    fn update_intent_status(&self, intent_id: &str, new_status: IntentStatus) -> Result<()> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        diesel::update(shadow_intents.filter(id.eq(intent_id)))
            .set((
                status.eq(new_status.as_str()),
                updated_at.eq(Utc::now().timestamp()),
            ))
            .execute(&mut conn)
            .context("Failed to update shadow intent status")?;

        Ok(())
    }

    fn update_intent_near_status(&self, intent_id: &str, new_near_status: &str) -> Result<()> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        diesel::update(shadow_intents.filter(id.eq(intent_id)))
            .set((
                near_status.eq(Some(new_near_status)),
                updated_at.eq(Utc::now().timestamp()),
            ))
            .execute(&mut conn)
            .context("Failed to update near status")?;

        Ok(())
    }

    fn update_intent_deposit_address(&self, intent_id: &str, addr: &str) -> Result<()> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        diesel::update(shadow_intents.filter(id.eq(intent_id)))
            .set((
                deposit_address.eq(Some(addr)),
                updated_at.eq(Utc::now().timestamp()),
            ))
            .execute(&mut conn)
            .context("Failed to update deposit address")?;

        Ok(())
    }

    fn update_intent_dest_tx(&self, intent_id: &str, tx_hash: &str) -> Result<()> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        diesel::update(shadow_intents.filter(id.eq(intent_id)))
            .set((
                dest_tx_hash.eq(Some(tx_hash)),
                updated_at.eq(Utc::now().timestamp()),
            ))
            .execute(&mut conn)
            .context("Failed to update dest tx hash")?;

        Ok(())
    }

    fn update_intent_settle_tx(&self, intent_id: &str, tx_hash: &str) -> Result<()> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        diesel::update(shadow_intents.filter(id.eq(intent_id)))
            .set((
                settle_tx_hash.eq(Some(tx_hash)),
                updated_at.eq(Utc::now().timestamp()),
            ))
            .execute(&mut conn)
            .context("Failed to update settle tx hash")?;

        Ok(())
    }

    fn update_intent_source_settle_tx(&self, intent_id: &str, tx_hash: &str) -> Result<()> {
        use schema::shadow_intents::dsl::*;

        let mut conn = self.get_connection()?;

        diesel::update(shadow_intents.filter(id.eq(intent_id)))
            .set((
                source_settle_tx_hash.eq(Some(tx_hash)),
                updated_at.eq(Utc::now().timestamp()),
            ))
            .execute(&mut conn)
            .context("Failed to update source settle tx hash")?;

        Ok(())
    }

    fn get_latest_root(&self, name: &str) -> Result<Option<String>> {
        use schema::merkle_roots::dsl::*;

        let mut conn = self.get_connection()?;

        merkle_roots
            .filter(tree_name.eq(name))
            .order(root_id.desc())
            .select(root)
            .first::<String>(&mut conn)
            .optional()
            .context("Failed to get latest root")
    }

    fn save_root(&self, name: &str, new_root: &str, count: u64) -> Result<()> {
        use schema::merkle_roots;

        let mut conn = self.get_connection()?;

        diesel::insert_into(merkle_roots::table)
            .values((
                merkle_roots::tree_name.eq(name),
                merkle_roots::root.eq(new_root),
                merkle_roots::leaf_count.eq(count as i64),
                merkle_roots::created_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .context("Failed to save merkle root")?;

        Ok(())
    }

    fn get_leaves(&self, name: &str) -> Result<Vec<String>> {
        use schema::merkle_leaves::dsl::*;

        let mut conn = self.get_connection()?;

        merkle_leaves
            .filter(tree_name.eq(name))
            .order(leaf_id.asc())
            .select(leaf)
            .load::<String>(&mut conn)
            .context("Failed to get merkle leaves")
    }

    fn add_leaf(&self, name: &str, leaf_val: &str) -> Result<()> {
        use schema::merkle_leaves;

        let mut conn = self.get_connection()?;

        diesel::insert_into(merkle_leaves::table)
            .values((
                merkle_leaves::tree_name.eq(name),
                merkle_leaves::leaf.eq(leaf_val),
                merkle_leaves::created_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .context("Failed to add merkle leaf")?;

        Ok(())
    }

    fn log_transaction(
        &self,
        intent_id_val: &str,
        chain_val: ChainId,
        tx_type_val: &str,
        tx_hash_val: &str,
        status_val: &str,
    ) -> Result<()> {
        use schema::transaction_logs;

        let mut conn = self.get_connection()?;

        diesel::insert_into(transaction_logs::table)
            .values((
                transaction_logs::intent_id.eq(intent_id_val),
                transaction_logs::chain.eq(chain_val.as_chain_id_str()),
                transaction_logs::tx_type.eq(tx_type_val),
                transaction_logs::tx_hash.eq(tx_hash_val),
                transaction_logs::status.eq(status_val),
                transaction_logs::created_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .context("Failed to log transaction")?;

        Ok(())
    }
}

// ==========================================================
//  DIESEL QUERYABLE STRUCTS
// ==========================================================

#[derive(Queryable, Selectable)]
#[diesel(table_name = schema::shadow_intents)]
struct DbShadowIntent {
    id: String,
    commitment: String,
    nullifier_hash: String,
    view_key: String,
    near_intents_id: String,
    source_chain: String,
    dest_chain: String,
    encrypted_recipient: String,
    token: String,
    amount: String,
    status: String,
    deposit_address: Option<String>,
    near_correlation_id: Option<String>,
    near_status: Option<String>,
    dest_tx_hash: Option<String>,
    settle_tx_hash: Option<String>,
    source_settle_tx_hash: Option<String>,
    encrypted_secret: Option<String>,
    encrypted_nullifier: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl From<DbShadowIntent> for ShadowIntent {
    fn from(db: DbShadowIntent) -> Self {
        ShadowIntent {
            id: db.id,
            commitment: db.commitment,
            nullifier_hash: db.nullifier_hash,
            view_key: db.view_key,
            near_intents_id: db.near_intents_id,
            source_chain: ChainId::from_name(&db.source_chain),
            dest_chain: ChainId::from_name(&db.dest_chain),
            encrypted_recipient: db.encrypted_recipient,
            token: db.token,
            amount: db.amount,
            status: IntentStatus::from_str(&db.status).unwrap_or(IntentStatus::Failed),
            deposit_address: db.deposit_address,
            near_correlation_id: db.near_correlation_id,
            near_status: db.near_status,
            dest_tx_hash: db.dest_tx_hash,
            settle_tx_hash: db.settle_tx_hash,
            source_settle_tx_hash: db.source_settle_tx_hash,
            encrypted_secret: db.encrypted_secret,
            encrypted_nullifier: db.encrypted_nullifier,
            created_at: db.created_at as u64,
            updated_at: db.updated_at as u64,
        }
    }
}
