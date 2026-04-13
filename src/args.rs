use cu::pre::*;

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
    path: String,
    /// Port to serve the content
    #[clap(short, long, default_value = "8241")]
    port: u16,

    /// Additional (source) directories to watch for changes.
    /// 
    /// Usually used together with a build command. When changes are
    /// detected in these paths, the build command will run
    /// and the page will reload.
    #[clap(short, long)]
    watch: Vec<String>,

    #[clap(flatten)]
    #[as_ref]
    flags: cu::cli::Flags,

    /// Command to run to rebuild the contents.
    ///
    /// The command should process any files watched by --watch
    /// and put the output in the primary path being watched by this tool.
    command: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn default_flags() -> cu::cli::Flags {
        cu::cli::Flags::try_parse_from([""]).unwrap()
    }

    fn verbose_flags() -> cu::cli::Flags {
        cu::cli::Flags::try_parse_from(["", "-v"]).unwrap()
    }

    #[test]
    fn test_path_only() {
        assert_eq!(
            Args::try_parse_from(["", "."]).unwrap(),
            Args {
                path: ".".into(),
                port: 8241,
                watch: vec![],
                flags: default_flags(),
                command: vec![],
            }
        );
    }

    #[test]
    fn test_with_command() {
        assert_eq!(
            Args::try_parse_from(["",".", "hello"]).unwrap(),
            Args {
                path: ".".into(),
                port: 8241,
                watch: vec![],
                flags: default_flags(),
                command: vec!["hello".into()],
            }
        );
    }

    #[test]
    fn test_with_watch_port_and_command() {
        assert_eq!(
            Args::try_parse_from(["",".", "hello", "-w", "foo", "-p", "12345", "bar"]).unwrap(),
            Args {
                path: ".".into(),
                port: 12345,
                watch: vec!["foo".into()],
                flags: default_flags(),
                command: vec!["hello".into(), "bar".into()],
            }
        );
    }

    #[test]
    fn test_verbose_flag() {
        assert_eq!(
            Args::try_parse_from(["",".", "hello", "-w", "foo", "-p", "12345", "bar", "-v"]).unwrap(),
            Args {
                path: ".".into(),
                port: 12345,
                watch: vec!["foo".into()],
                flags: verbose_flags(),
                command: vec!["hello".into(), "bar".into()],
            }
        );
    }
}
