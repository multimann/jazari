//! A small AVM1 disassembler, for tests, tooling and checking generated code.
//!
//! Function, `with` and `try` bodies are listed after their opening action
//! with a greater depth. Constant pool references are resolved, so a `Push`
//! of `constant3` shows the string itself.

use std::fmt;
use swf::avm1::read::Reader;
use swf::avm1::types::{Action, CatchVar, Value};

/// Errors of [`disassemble`].
#[derive(Debug, thiserror::Error)]
pub enum DisasmError {
    /// The bytes are not valid AVM1 actions.
    #[error("invalid AVM1 bytecode at offset {offset}: {message}")]
    Invalid {
        /// Byte offset of the action that could not be read.
        offset: usize,
        /// What went wrong.
        message: String,
    },
}

/// A decoded operand.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// A string, from the constant pool or inline.
    Str(String),
    /// A 32-bit integer.
    Int(i32),
    /// A number (`Float` or `Double` in the SWF).
    Number(f64),
    /// A boolean.
    Bool(bool),
    /// `null`.
    Null,
    /// `undefined`.
    Undefined,
    /// A register number.
    Register(u8),
    /// A constant pool index that is out of range.
    BadConstant(u16),
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Str(s) => write!(f, "{s:?}"),
            Operand::Int(i) => write!(f, "{i}"),
            Operand::Number(n) => write!(f, "{n:?}"),
            Operand::Bool(b) => write!(f, "{b}"),
            Operand::Null => f.write_str("null"),
            Operand::Undefined => f.write_str("undefined"),
            Operand::Register(r) => write!(f, "r{r}"),
            Operand::BadConstant(i) => write!(f, "<bad constant {i}>"),
        }
    }
}

/// One decoded action.
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    /// Byte offset in its block (a function body starts again at 0).
    pub offset: usize,
    /// 0 at the top level, 1 inside a function, `with` or `try` body, ...
    pub depth: usize,
    /// The action name, as in the `swf` crate (`Push`, `GetVariable`, ...).
    pub name: String,
    /// Pushed values, pool strings, parameter names or other operands.
    pub operands: Vec<Operand>,
    /// For `DefineFunction2`: the registers set when the function starts
    /// (preloaded `this`, `_root`, ... and parameters), with their names.
    pub registers: Vec<(u8, String)>,
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:indent$}{}", "", self.name, indent = self.depth * 4)?;
        for (i, operand) in self.operands.iter().enumerate() {
            f.write_str(if i == 0 { " " } else { ", " })?;
            write!(f, "{operand}")?;
        }
        Ok(())
    }
}

/// Decodes a DoAction (or button/clip action) payload.
pub fn disassemble(actions: &[u8], swf_version: u8) -> Result<Vec<Instruction>, DisasmError> {
    let mut out = Vec::new();
    let mut pool = Vec::new();
    decode_block(actions, swf_version, 0, &mut pool, &mut out)?;
    Ok(out)
}

/// The listing as text, one instruction per line.
pub fn listing(instructions: &[Instruction]) -> String {
    let mut text = String::new();
    for instruction in instructions {
        text.push_str(&instruction.to_string());
        text.push('\n');
    }
    text
}

fn decode_block(
    data: &[u8],
    version: u8,
    depth: usize,
    pool: &mut Vec<String>,
    out: &mut Vec<Instruction>,
) -> Result<(), DisasmError> {
    let encoding = swf::SwfStr::encoding_for_version(version);
    let text = |s: &swf::SwfStr| s.to_str_lossy(encoding).into_owned();
    let mut reader = Reader::new(data, version);
    while !reader.get_ref().is_empty() {
        let offset = data.len() - reader.get_ref().len();
        let action = reader.read_action().map_err(|e| DisasmError::Invalid {
            offset,
            message: e.to_string(),
        })?;
        let name = action_name(&action);
        let mut operands = Vec::new();
        let mut registers = Vec::new();
        let mut bodies: Vec<&[u8]> = Vec::new();
        match &action {
            Action::ConstantPool(cp) => {
                *pool = cp.strings.iter().map(|s| text(s)).collect();
                operands = pool.iter().cloned().map(Operand::Str).collect();
            }
            Action::Push(push) => {
                operands = push
                    .values
                    .iter()
                    .map(|value| match value {
                        Value::Undefined => Operand::Undefined,
                        Value::Null => Operand::Null,
                        Value::Bool(b) => Operand::Bool(*b),
                        Value::Int(i) => Operand::Int(*i),
                        Value::Float(x) => Operand::Number(f64::from(*x)),
                        Value::Double(x) => Operand::Number(*x),
                        Value::Str(s) => Operand::Str(text(s)),
                        Value::Register(r) => Operand::Register(*r),
                        Value::ConstantPool(i) => pool
                            .get(usize::from(*i))
                            .cloned()
                            .map_or(Operand::BadConstant(*i), Operand::Str),
                    })
                    .collect();
            }
            Action::DefineFunction(function) => {
                operands.push(Operand::Str(text(function.name)));
                operands.extend(function.params.iter().map(|p| Operand::Str(text(p))));
                bodies.push(function.actions);
            }
            Action::DefineFunction2(function) => {
                operands.push(Operand::Str(text(function.name)));
                operands.extend(function.params.iter().map(|p| Operand::Str(text(p.name))));
                registers = function_registers(function, &text);
                bodies.push(function.actions);
            }
            Action::With(with) => bodies.push(with.actions),
            Action::Try(t) => {
                if let Some((var, _)) = &t.catch_body {
                    operands.push(match var {
                        CatchVar::Var(name) => Operand::Str(text(name)),
                        CatchVar::Register(r) => Operand::Register(*r),
                    });
                }
                bodies.push(t.try_body);
                if let Some((_, body)) = &t.catch_body {
                    bodies.push(body);
                }
                if let Some(body) = t.finally_body {
                    bodies.push(body);
                }
            }
            Action::GetUrl(get_url) => {
                operands.push(Operand::Str(text(get_url.url)));
                operands.push(Operand::Str(text(get_url.target)));
            }
            Action::GotoLabel(label) => operands.push(Operand::Str(text(label.label))),
            Action::SetTarget(target) => operands.push(Operand::Str(text(target.target))),
            Action::GotoFrame(goto) => operands.push(Operand::Int(i32::from(goto.frame))),
            Action::If(jump) => operands.push(Operand::Int(i32::from(jump.offset))),
            Action::Jump(jump) => operands.push(Operand::Int(i32::from(jump.offset))),
            Action::StoreRegister(store) => operands.push(Operand::Register(store.register)),
            _ => {}
        }
        let is_end = matches!(action, Action::End);
        out.push(Instruction {
            offset,
            depth,
            name,
            operands,
            registers,
        });
        for body in bodies {
            decode_block(body, version, depth + 1, pool, out)?;
        }
        if is_end {
            break;
        }
    }
    Ok(())
}

/// What each register holds when a `DefineFunction2` function starts.
/// Preloaded values take registers 1, 2, ... in the order `this`,
/// `arguments`, `super`, `_root`, `_parent`, `_global` (SWF 19, page 87).
fn function_registers(
    function: &swf::avm1::types::DefineFunction2<'_>,
    text: &impl Fn(&swf::SwfStr) -> String,
) -> Vec<(u8, String)> {
    use swf::avm1::types::FunctionFlags as F;
    let preloads = [
        (F::PRELOAD_THIS, "this"),
        (F::PRELOAD_ARGUMENTS, "arguments"),
        (F::PRELOAD_SUPER, "super"),
        (F::PRELOAD_ROOT, "_root"),
        (F::PRELOAD_PARENT, "_parent"),
        (F::PRELOAD_GLOBAL, "_global"),
    ];
    let mut registers = Vec::new();
    let mut next = 1u8;
    for (flag, name) in preloads {
        if function.flags.contains(flag) {
            registers.push((next, name.to_owned()));
            next = next.saturating_add(1);
        }
    }
    for param in &function.params {
        if let Some(register) = param.register_index {
            registers.push((register.get(), text(param.name)));
        }
    }
    registers
}

/// The variant name of an action (`Push`, `GetVariable`, ...).
fn action_name(action: &Action<'_>) -> String {
    let debug = format!("{action:?}");
    let end = debug
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(debug.len());
    debug[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_pool_push_and_function_bodies() {
        // ConstantPool "a"; Push c0; DefineFunction "f"(x) { Push 1; GetVariable; End };
        // Trace; End
        let bytes = [
            0x88, 0x04, 0x00, 0x01, 0x00, b'a', 0x00, // ConstantPool ["a"]
            0x96, 0x02, 0x00, 0x08, 0x00, // Push constant 0
            0x9b, 0x08, 0x00, b'f', 0x00, 0x01, 0x00, b'x', 0x00, 0x0a,
            0x00, // DefineFunction f(x)
            0x96, 0x05, 0x00, 0x07, 0x01, 0x00, 0x00, 0x00, // Push 1
            0x1c, // GetVariable
            0x00, // End (of the body)
            0x26, // Trace
            0x00, // End
        ];
        let instructions = disassemble(&bytes, 8).expect("valid");
        assert_eq!(
            listing(&instructions),
            "ConstantPool \"a\"\nPush \"a\"\nDefineFunction \"f\", \"x\"\n    Push 1\n    GetVariable\n    End\nTrace\nEnd\n"
        );
    }

    #[test]
    fn truncated_bytes_are_an_error() {
        let error = disassemble(&[0x96, 0x05, 0x00, 0x07], 8).expect_err("truncated");
        assert!(
            matches!(error, DisasmError::Invalid { offset: 0, .. }),
            "{error}"
        );
    }
}
