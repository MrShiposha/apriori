use postgres::{
    NoTls
};
use super::Result;

pub struct StorageManager {
    psql: postgres::Client
}

impl StorageManager {
    pub fn connect(connection_string: &str) -> Result<Self> {
        Ok(Self {
            psql: postgres::Client::connect(connection_string, NoTls)?
        })
    }

    pub fn setup_schema(&mut self, session_max_hang_time: chrono::Duration) -> Result<()> {
        Ok(self.psql.batch_execute(
            format! {
                include_str!("sql/setup_schema.sql"),
                session_max_hang_time = session_max_hang_time.num_seconds()
            }.as_ref()
        )?)
    }

    pub fn list_sessions(&mut self) -> Result<()> {
        for row in self.psql.query(
            r#"
                SELECT session_name, last_access 
                FROM apriori.session
                WHERE session_name IS NOT NULL
                ORDER BY last_access
            "#, 
            &[]
        )? {
            let name: &str = row.get(0);
            let last_access: chrono::DateTime<chrono::Local> = row.get(1);
            println!("\t{} [last access {}]", name, last_access);
        }

        Ok(())
    }
}