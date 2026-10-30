//! `pscat chalk`: LaTeX in, a chalkboard out (issue #555).
//!
//! A thin front over `pscat::chalk`, the driver generator: this file only
//! reads the arguments, the LaTeX and the config, and writes the bytes
//! `pscat::chalk::render` returns, so the CLI and every other caller of the
//! library (the MCP server, a browser demo) draw the same board.

use std::io::Read;

use pscat::Limits;
use pscat::chalk::{self, Config, Format};
use serde_json::{Value, json};

use super::{Args, Failure, number, output_format, take_limit_flag};

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
      --max-steps N     stop the render after N interpreter steps (0 = no limit)
      --max-time SECS   stop the render after SECS of wall-clock time (0 = no limit)
      --max-memory MIB  stop a render that grows the heap by more than MIB
                        (0 = no limit). The board is one run under them; a stop
                        writes no file and exits non-zero with `limitexceeded`
  -h, --help          this text

Write the LaTeX as you would in a document: single backslashes, \\ between
rows, & to align them; quote it for the shell. A bracket right after \\ is
TeX's row spacing (\\[2pt]) only when it holds a length; otherwise
(\\ [x,y] = 0) it starts the next row, and \left[ ... \right] is always a
bracket. Flags override the config. A formula that starts with "-" goes after
"--". `pscat chalk --help` also lists the LaTeX it knows."#;

/// The LaTeX `pscat chalk` knows (mathkit's subset), printed by `--help`
/// only, so a usage error stays short. An installed pscat has no
/// lib/mathkit.ps to read (issue #934).
const SUBSET: &str = r#"Supported LaTeX (anything else fails with one line naming it):
  scripts       x^2  x_i  x_i^2  f'
  fractions     \frac \dfrac \tfrac \binom \sqrt{x} \sqrt[n]{x}
  big operators \sum \prod \int \iint \iiint \oint \limits \nolimits
  names         \lim \max \min \sup \inf \det \gcd \Pr \sin \cos \tan \log
                \ln \exp ... \operatorname{...}
  delimiters    \left( ... \right) with ( ) [ ] \{ \} | \| \langle \rangle .
                \big \Big \bigg \Bigg (and their l and r forms)
  environments  matrix pmatrix bmatrix Bmatrix vmatrix cases; the whole input
                may sit in align, gather, equation, $...$, \[...\] ...
  accents       \vec \hat \bar \dot \ddot \tilde \overline \underline \boxed
  braces        \overbrace{}^{} \underbrace{}_{} \overset \underset \stackrel
                \substack
  text, fonts   \text{} \mathrm{} \mathbf{} \mathbb{} \mathcal{} (the last
                two: capital letters only)
  spacing       \, \: \; \! \  \quad \qquad ~
  styles        \displaystyle \textstyle \scriptstyle \scriptscriptstyle
  Greek         \alpha ... \omega  \Gamma ... \Omega  \varphi \vartheta
                \varepsilon \varrho \varpi \varsigma \varkappa
  relations     = < > \le \ge \ne \approx \equiv \sim \simeq \cong \propto
                \in \notin \subset \subseteq \supset \supseteq \perp
                \parallel \mid \to \gets \mapsto \Rightarrow \Leftrightarrow
                \iff \leftrightarrow \vdash \nvdash \models \not
  operators     + - \pm \mp \times \div \cdot \circ \ast \star \setminus
                \cup \cap \oplus \otimes \wedge \land \vee \lor \dagger
                \ddagger \bmod \pmod \mod \pod
  logic         \forall \exists \nexists \neg \lnot \top \bot \therefore
                \because
  symbols       \infty \partial \nabla \hbar \ell \emptyset \varnothing
                \angle \triangle \Re \Im \prime \ldots \cdots
  Unicode       Greek and math symbols typed directly (α ∞ ≤ ∂ ∧ ℝ ...)"#;

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
    /// `--max-steps`, `--max-time`, `--max-memory` (issue #701).
    limits: Limits,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(opts) = parse(args).map_err(usage)? else {
        println!("{USAGE}\n\n{SUBSET}");
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
    let bytes = chalk::render_with_limits(&latex, &cfg, format, opts.limits).map_err(err)?;
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
        limits: Limits::none(),
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
            _ if take_limit_flag(&mut o.limits, &arg, &mut args)? => {}
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
