//! MyBatis Plus: statement generation, the `@Param` quickfix and SQL-log
//! conversion. Navigation (gutter, Go to Definition, completion) is the
//! official `openidea.mybatis` plugin's job — this plugin adds what it lacks.

use openidea_plugin::{Error, Invocation, Plugin, register_plugin, host, NotificationLevel};

#[derive(Default)]
struct MybatisPlus;

impl Plugin for MybatisPlus {
    fn run_command(&mut self, id: &str, _ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        match id {
            "generate-statement" => {
                host::notify(NotificationLevel::Info, "MyBatis Plus", "generation lands next");
                Ok(None)
            }
            "convert-sql-log" => {
                host::notify(NotificationLevel::Info, "MyBatis Plus", "conversion lands next");
                Ok(None)
            }
            other => Err(Error::NotFound(format!("command {other}"))),
        }
    }
}

register_plugin!(MybatisPlus);
