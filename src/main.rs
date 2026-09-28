use std::{env, process};

const HELP: &str = "\
Hello world CLI

Usage: rehab [OPTIONS]

Options:
  -n, --name <NAME>  Name to greet
  -h, --help         Print help
";

fn greeting(name: Option<&str>) -> String {
    format!("Hello, {}", name.unwrap_or("World"))
}

fn run<I>(args: I) -> Result<String, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    let mut name = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(HELP.to_string()),
            "-n" | "--name" => {
                let value = args
                    .next()
                    .ok_or_else(|| "error: missing value for --name".to_string())?;
                name = Some(value);
            }
            _ => return Err(format!("error: unexpected argument '{arg}'")),
        }
    }

    Ok(greeting(name.as_deref()))
}

fn main() {
    match run(env::args()) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HELP, greeting, run};

    #[test]
    fn defaults_to_world() {
        assert_eq!(greeting(None), "Hello, World");
    }

    #[test]
    fn uses_provided_name() {
        assert_eq!(greeting(Some("Copilot")), "Hello, Copilot");
    }

    #[test]
    fn prints_help() {
        let output = run(vec!["rehab".into(), "--help".into()]).unwrap();
        assert_eq!(output, HELP);
    }
}
