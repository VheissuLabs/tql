use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Driver {
    #[default]
    Sqlite,
    Mysql,
    Pgsql,
    Sqlsrv,
}

impl Driver {
    pub const ALL: [Driver; 4] = [Driver::Sqlite, Driver::Mysql, Driver::Pgsql, Driver::Sqlsrv];

    pub fn parse(name: &str) -> Option<Driver> {
        match name.trim().to_lowercase().as_str() {
            "sqlite" => Some(Driver::Sqlite),
            "mysql" | "mariadb" => Some(Driver::Mysql),
            "pgsql" => Some(Driver::Pgsql),
            "sqlsrv" => Some(Driver::Sqlsrv),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Driver::Sqlite => "sqlite",
            Driver::Mysql => "mysql",
            Driver::Pgsql => "pgsql",
            Driver::Sqlsrv => "sqlsrv",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Driver::Sqlite => "SQLite",
            Driver::Mysql => "MySQL",
            Driver::Pgsql => "Postgres",
            Driver::Sqlsrv => "SQL Server",
        }
    }

    pub fn default_port(self) -> Option<u16> {
        match self {
            Driver::Sqlite => None,
            Driver::Mysql => Some(3306),
            Driver::Pgsql => Some(5432),
            Driver::Sqlsrv => Some(1433),
        }
    }

    pub fn is_server(self) -> bool {
        self != Driver::Sqlite
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
