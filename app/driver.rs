use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Driver {
    #[default]
    Sqlite,
    Mysql,
    Pgsql,
    Sqlsrv,
    Duckdb,
    Libsql,
    Clickhouse,
    Snowflake,
    Redis,
}

impl Driver {
    pub const ALL: [Driver; 9] = [
        Driver::Sqlite,
        Driver::Mysql,
        Driver::Pgsql,
        Driver::Sqlsrv,
        Driver::Duckdb,
        Driver::Libsql,
        Driver::Clickhouse,
        Driver::Snowflake,
        Driver::Redis,
    ];

    pub fn parse(name: &str) -> Option<Driver> {
        match name.trim().to_lowercase().as_str() {
            "sqlite" => Some(Driver::Sqlite),
            "mysql" | "mariadb" => Some(Driver::Mysql),
            "pgsql" => Some(Driver::Pgsql),
            "sqlsrv" => Some(Driver::Sqlsrv),
            "duckdb" => Some(Driver::Duckdb),
            "libsql" | "turso" => Some(Driver::Libsql),
            "clickhouse" => Some(Driver::Clickhouse),
            "snowflake" => Some(Driver::Snowflake),
            "redis" | "valkey" => Some(Driver::Redis),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Driver::Sqlite => "sqlite",
            Driver::Mysql => "mysql",
            Driver::Pgsql => "pgsql",
            Driver::Sqlsrv => "sqlsrv",
            Driver::Duckdb => "duckdb",
            Driver::Libsql => "libsql",
            Driver::Clickhouse => "clickhouse",
            Driver::Snowflake => "snowflake",
            Driver::Redis => "redis",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Driver::Sqlite => "SQLite",
            Driver::Mysql => "MySQL",
            Driver::Pgsql => "Postgres",
            Driver::Sqlsrv => "SQL Server",
            Driver::Duckdb => "DuckDB",
            Driver::Libsql => "libSQL",
            Driver::Clickhouse => "ClickHouse",
            Driver::Snowflake => "Snowflake",
            Driver::Redis => "Redis / Valkey",
        }
    }

    pub fn default_port(self) -> Option<u16> {
        match self {
            Driver::Sqlite | Driver::Duckdb | Driver::Libsql | Driver::Snowflake => None,
            Driver::Mysql => Some(3306),
            Driver::Pgsql => Some(5432),
            Driver::Sqlsrv => Some(1433),
            Driver::Clickhouse => Some(8123),
            Driver::Redis => Some(6379),
        }
    }

    pub fn is_server(self) -> bool {
        !self.is_file()
    }

    pub fn is_file(self) -> bool {
        matches!(self, Driver::Sqlite | Driver::Duckdb)
    }

    pub fn edits_rows(self) -> bool {
        !matches!(self, Driver::Clickhouse)
    }

    pub fn has_databases(self) -> bool {
        matches!(
            self,
            Driver::Mysql
                | Driver::Pgsql
                | Driver::Sqlsrv
                | Driver::Clickhouse
                | Driver::Snowflake
                | Driver::Redis
        )
    }
}

impl fmt::Display for Driver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_name_tql_has_saved_a_driver_under() {
        assert_eq!(Driver::parse("mariadb"), Some(Driver::Mysql));
        assert_eq!(Driver::parse(" PGSQL "), Some(Driver::Pgsql));
        assert_eq!(Driver::parse("oracle"), None);
        for driver in Driver::ALL {
            assert_eq!(Driver::parse(driver.name()), Some(driver));
        }
    }
}
