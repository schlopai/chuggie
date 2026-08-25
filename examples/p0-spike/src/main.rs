#![no_std]
#![no_main]
#![allow(unused, non_snake_case)]

//! P0a de-risking spike — a hand-written mock of what the tish `Gba` emit mode
//! will generate. Every construct here is one the real codegen prelude/runtime
//! depends on; a green `thumbv4t-none-eabi` build that boots in mGBA proves the
//! no_std + no-atomics port is viable before we touch the tish compiler.
//!
//! Claims under test:
//!   1. `Arc<T> = Rc<T>` type alias absorbs emitted `Arc::from` / `Arc::clone`.
//!   2. `Rc<str>`-keyed hashbrown map with foldhash (NOT ahash, which needs atomics).
//!   3. `Rc<dyn Fn(&[Value]) -> Value>` native-fn objects (what `Value::native` becomes).
//!   4. libm-routed f64 soft-float math.
//!   5. `run() -> Result<(), Box<dyn core::error::Error>>` survives no_std.
//!   6. `#[agb::entry]` entry + `agb::println!` (console.log sink).

extern crate alloc;

use core::cell::RefCell;
use core::fmt;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};

use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;

// ── Zero-churn trick #1: the codegen emits `Arc::from(..)` / `Arc::clone(..)`
//    everywhere for interned object keys. On GBA the facade aliases Arc→Rc, so
//    all those sites compile unchanged against single-threaded Rc.
pub type Arc<T> = alloc::rc::Rc<T>;

// ── The dynamic value core (mirrors tish_core::Value's load-bearing variants).
//    Rc/RefCell are alloc-only (fine); the parts that DON'T port are the string
//    repr (ArcStr) and the hasher (ahash) — both swapped below.
type NativeFn = Rc<dyn Fn(&[Value]) -> Value>;
type ObjectMap = HashMap<Arc<str>, Value, FxBuildHasher>;

#[derive(Clone)]
enum Value {
    Number(f64),
    Str(Arc<str>),
    Bool(bool),
    Null,
    Array(Rc<RefCell<Vec<Value>>>),
    Object(Rc<RefCell<ObjectMap>>),
    Function(NativeFn),
}

impl Value {
    fn native<F: Fn(&[Value]) -> Value + 'static>(f: F) -> Value {
        Value::Function(Rc::new(f))
    }

    fn object() -> Value {
        Value::Object(Rc::new(RefCell::new(ObjectMap::with_hasher(FxBuildHasher))))
    }

    fn display(&self) -> String {
        match self {
            Value::Number(n) => format_number(*n),
            Value::Str(s) => s.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Array(a) => {
                let items: Vec<String> = a.borrow().iter().map(|v| v.display()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Object(_) => "[object Object]".to_string(),
            Value::Function(_) => "[function]".to_string(),
        }
    }
}

// f64 formatting without std — the real runtime uses ryu/itoa; here a crude
// path is enough to prove soft-float + alloc formatting link on-device.
fn format_number(n: f64) -> String {
    if n == libm::trunc(n) && libm::fabs(n) < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

// ── console.log sink → agb::println! → mGBA debug log.
fn console_log(v: &Value) {
    agb::println!("{}", v.display());
}

// ── A tiny error type over core::error::Error, proving the `?`/Box<dyn Error>
//    machinery the codegen emits for `run()` survives no_std.
#[derive(Debug)]
struct TishError(String);
impl fmt::Display for TishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TishError: {}", self.0)
    }
}
impl core::error::Error for TishError {}

/// The body every tish program compiles into.
fn run() -> Result<(), Box<dyn core::error::Error>> {
    // (1) Arc→Rc alias, inferred and explicit key construction.
    let k_hp: Arc<str> = Arc::from("hp");
    let k_name = Arc::<str>::from("name");

    // (2) Rc<str>-keyed foldhash map — the ObjectMap replacement.
    let player = Value::object();
    if let Value::Object(map) = &player {
        let mut m = map.borrow_mut();
        m.insert(Arc::clone(&k_hp), Value::Number(100.0));
        m.insert(Arc::clone(&k_name), Value::Str(Arc::from("Mara")));
    }

    // (3) native fn objects (what builtins/closures lower to).
    let sqrt = Value::native(|args| match args {
        [Value::Number(n), ..] => Value::Number(libm::sqrt(*n)),
        _ => Value::Null,
    });

    // (4) libm soft-float math through the dynamic path.
    let hp = match &player {
        Value::Object(m) => m.borrow().get(&k_hp).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let dmg = Value::Number(libm::floor(libm::sin(1.0) * 30.0));
    let remaining = match (hp, &dmg) {
        (Value::Number(a), Value::Number(b)) => Value::Number(a - b),
        _ => return Err(Box::new(TishError("type error".into()))),
    };

    // (5) array + call through a native fn.
    let list = Value::Array(Rc::new(RefCell::new(vec![
        Value::Number(9.0),
        remaining.clone(),
        Value::Str(Arc::from("ok")),
    ])));

    let root = if let Value::Function(f) = &sqrt {
        f(&[Value::Number(1764.0)])
    } else {
        Value::Null
    };

    // (6) log it all — visible in mGBA stdout.
    console_log(&Value::Str(Arc::from("p0-spike: dynamic Value core is alive")));
    console_log(&player.display_object_for_test(&k_name));
    console_log(&remaining);
    console_log(&list);
    console_log(&root);
    console_log(&dmg);
    Ok(())
}

impl Value {
    // helper so the test can read a named field back out
    fn display_object_for_test(&self, key: &Arc<str>) -> Value {
        match self {
            Value::Object(m) => m.borrow().get(key).cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        }
    }
}

#[agb::entry]
fn agb_main(_gba: agb::Gba) -> ! {
    // Mirrors the emitted `agb_main`: (init gba) → run() → halt.
    match run() {
        Ok(()) => agb::println!("p0-spike: run() ok"),
        Err(e) => agb::println!("p0-spike: run() error: {}", e),
    }
    loop {
        agb::halt();
    }
}
