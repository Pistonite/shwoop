use cu::pre::*;

/// LOGO made with typez
static LOGO: &str = r#" ______ __  __ __     __ ______ ______ ______  
/\  ___\\ \_\ \\ \  _ \ \\  __ \\  __ \\  == \ 
\ \___  \\  __ \\ \/ ".\ \\ \/\ \\ \/\ \\  _-/ 
 \/\_____\\_\ \_\\__/".~\_\\_____\\_____\\_\    
  \/_____//_/\/_//_/   \/_//_____//_____//_/    "#;

/// Tool for watch and hot-reload static webpages
#[derive(Debug, PartialEq, clap::Parser, AsRef)]
#[clap(
    before_help = LOGO,
    version
)]
pub struct Args {
    /// Primary (output) path to watch for changes and reload the page.
    pub path: String,

    /// Port to serve the content
    #[clap(short, long, default_value = "8241")]
    pub port: u16,

    /// Host the server on the local network. By default, it will only host on localhost
    #[clap(long)]
    pub host: bool,

    /// Serve the content without watching for changes or hot-reloading
    #[clap(long, conflicts_with = "watch")]
    pub raw: bool,

    /// Additional (source) directories to watch for changes.
    ///
    /// Usually used together with a build command. When changes are
    /// detected in these paths, the build command will run
    /// and the page will reload.
    #[clap(short, long)]
    pub watch: Vec<String>,

    #[clap(flatten)]
    #[as_ref]
    flags: cu::cli::Flags,

    /// Command to run to rebuild the contents.
    ///
    /// The command should process any files watched by --watch
    /// and put the output in the primary path being watched by this tool.
    pub command: Vec<String>,
}

pub struct LogConfig;
impl cu::cli::LogConfig for LogConfig {
    fn process(&self, record: &cu::lv::LogRecord) -> (cu::lv::Lv, bool) {
        if let Some(m) = record.module_path() {
            if m.starts_with("watchexec") || m.starts_with("actix") {
                let level = record.level();
                let is_info = record.level() == cu::lv::LogLevel::Info;
                return (if is_info { cu::lv::D } else { level.into() }, true);
            }
        }
        cu::cli::DefaultLogConfig.process(record)
    }
}
