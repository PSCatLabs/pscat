//! `pscat chalk`: LaTeX in, a chalkboard out (issue #555).
//!
//! A thin front over `pscat::chalk`, the driver generator: this file only
//! reads the arguments, the LaTeX and the config, and writes the bytes
//! `pscat::chalk::render` returns, so the CLI and every other caller of the
//! library (the MCP server, a browser demo) draw the same board.

use std::io::Read;

use pscat::chalk::{self, Config, Format};
use serde_json::{Value, json};

use super::{Args, Failure, number, output_format};

const USAGE: &str = r#"usage: pscat chalk [options] 'LATEX'
       pscat chalk [options] -            (LaTeX from stdin)
       pscat chalk [options] --file eq.tex

  -o, --out PATH      output .png, .svg or .pdf (default chalkboard.png)
      --config FILE   JSON config (layout, chalk, board, seed; see the guide)
      --json TEXT     the same JSON inline; laid over --config
      --seed N        seed for the layout, chalk and board (default 1)
      --size PT       writing size in points, before any shrink to fit (default 64)
      --irregular R   0 = a clean, untilted layout; 1 a steady hand (default 1)
      --color NAME    white | yellow | pink | blue | green | orange (default white)
      --width PT      page width in points (default 800)
      --height PT     page height in points (default 400)
      --dpi N         PNG resolution (default 144)
      --no-board      chalk alone on a transparent page
      --print-ps      print the generated PostScript instead of rendering
  -h, --help          this text

Write the LaTeX as you would in a document: single backslashes, \\ between
rows, & to align them; quote it for the shell. The supported subset is
mathkit's (the top of lib/mathkit.ps). Flags override the config. A formula
that starts with "-" goes after "--"."#;

enum Input {
    Arg(String),
    Stdin,
    File(String),
}

struct Opts {
    input: Option<Input>,
    out: String,
    config: Option<String>,
    json: Option<String>,
    overrides: Vec<(&'static str, &'static str, Value)>,
    print_ps: bool,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(opts) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };
    let latex = match opts.input {
        None => return Err(usage("no LaTeX given".into())),
        Some(Input::Arg(s)) => s,
        Some(Input::Stdin) => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| Failure::Msg(format!("cannot read stdin: {e}")))?;
            s
        }
        Some(Input::File(p)) => std::fs::read_to_string(&p)
            .map_err(|e| Failure::Msg(format!("cannot read {p}: {e}")))?,
    };

    let mut cfg = Config::default();
    let err = |e: chalk::ChalkError| Failure::Msg(e.to_string());
    if let Some(path) = &opts.config {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Failure::Msg(format!("cannot read {path}: {e}")))?;
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| Failure::Msg(format!("{path}: not valid JSON: {e}")))?;
        cfg.apply(&v).map_err(err)?;
    }
    if let Some(text) = &opts.json {
        let v: Value = serde_json::from_str(text)
            .map_err(|e| Failure::Msg(format!("--json: not valid JSON: {e}")))?;
        cfg.apply(&v).map_err(err)?;
    }
    for (group, key, value) in opts.overrides {
        let v = if group.is_empty() {
            json!({ key: value })
        } else {
            json!({ group: { key: value } })
        };
        cfg.apply(&v).map_err(err)?;
    }

    if opts.print_ps {
        let d = chalk::driver(&latex, &cfg).map_err(err)?;
        print!("{}", d.source);
        return Ok(());
    }
    let format = match output_format(&opts.out).map_err(usage)? {
        super::Format::Png => Format::Png,
        super::Format::Svg => Format::Svg,
        super::Format::Pdf => Format::Pdf,
    };
    // Nothing is written unless the board rendered: a TeX error leaves no file.
    let bytes = chalk::render(&latex, &cfg, format).map_err(err)?;
    std::fs::write(&opts.out, bytes)
        .map_err(|e| Failure::Msg(format!("cannot write {}: {e}", opts.out)))?;
    println!("pscat: wrote {}", opts.out);
    Ok(())
}

fn parse(mut args: Args) -> Result<Option<Opts>, String> {
    let mut o = Opts {
        input: None,
        out: "chalkboard.png".into(),
        config: None,
        json: None,
        overrides: Vec::new(),
        print_ps: false,
    };
    let set_input = |o: &mut Opts, i: Input| {
        if o.input.is_some() {
            Err("give the LaTeX once (an argument, - or --file)".to_string())
        } else {
            o.input = Some(i);
            Ok(())
        }
    };
    // a number flag: checked as a plain decimal, then handed to the config
    // as JSON, whose own checks give the range
    let numeric = |flag: &str, v: String| -> Result<Value, String> {
        number(flag, &v)?;
        serde_json::from_str::<Value>(&v)
            .or_else(|_| v.parse::<f64>().map(Value::from))
            .map_err(|_| format!("{flag} needs a number, got: {v}"))
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => o.out = args.value(&arg)?,
            "--config" => o.config = Some(args.value(&arg)?),
            "--json" => o.json = Some(args.value(&arg)?),
            "--file" => {
                let f = args.value(&arg)?;
                set_input(&mut o, Input::File(f))?;
            }
            "--seed" => {
                let v = numeric(&arg, args.value(&arg)?)?;
                o.overrides.push(("", "seed", v));
            }
            "--size" => {
                let v = numeric(&arg, args.value(&arg)?)?;
                o.overrides.push(("layout", "size", v));
            }
            "--irregular" => {
                let v = numeric(&arg, args.value(&arg)?)?;
                o.overrides.push(("layout", "irregular", v));
            }
            "--width" => {
                let v = numeric(&arg, args.value(&arg)?)?;
                o.overrides.push(("layout", "width", v));
            }
            "--height" => {
                let v = numeric(&arg, args.value(&arg)?)?;
                o.overrides.push(("layout", "height", v));
            }
            "--dpi" => {
                let v = numeric(&arg, args.value(&arg)?)?;
                o.overrides.push(("layout", "dpi", v));
            }
            "--color" => {
                let v = args.value(&arg)?;
                o.overrides.push(("chalk", "color", Value::from(v)));
            }
            "--no-board" => o.overrides.push(("", "board", Value::Bool(false))),
            "--print-ps" => o.print_ps = true,
            "-" => set_input(&mut o, Input::Stdin)?,
            "--" => {
                for rest in args.remaining() {
                    set_input(&mut o, Input::Arg(rest))?;
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option: {s}"));
            }
            _ => set_input(&mut o, Input::Arg(arg))?,
        }
    }
    Ok(Some(o))
}
