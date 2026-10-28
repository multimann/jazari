//! The stage build's frame 1 script: `stop()` plus a `jz_goto` callback that
//! the editor calls (through `Player::call_internal_interface`) to show a
//! frame.

use crate::backend;
use crate::disasm::{Instruction, Operand, disassemble};
use crate::{CompilerOptions, Diagnostic, Origin, Severity};
use std::sync::OnceLock;
use swf::SwfStr;
use swf::avm1::types::{Action, ConstantPool, DefineFunction, Push, Value};

/// The source of the stage hook.
pub const STAGE_GOTO_SOURCE: &str = "stop();\n\
     flash.external.ExternalInterface.addCallback(\"jz_goto\", null, function (f) { _root.gotoAndStop(f); });\n";

/// DoAction bytes for frame 1 of the stage build:
///
/// ```actionscript
/// stop();
/// flash.external.ExternalInterface.addCallback("jz_goto", null, function (f) { _root.gotoAndStop(f); });
/// ```
///
/// Compiled with Rascal and checked by disassembly (`flash` is read with
/// `GetVariable` and `external`, `ExternalInterface` with `GetMember`). If
/// that check ever fails, the same code is assembled directly with the `swf`
/// crate's AVM1 writer. The result is computed once.
pub fn stage_goto_hook() -> Result<Vec<u8>, Vec<Diagnostic>> {
    static HOOK: OnceLock<Result<Vec<u8>, Vec<Diagnostic>>> = OnceLock::new();
    HOOK.get_or_init(build_stage_goto_hook).clone()
}

fn build_stage_goto_hook() -> Result<Vec<u8>, Vec<Diagnostic>> {
    let compiled = backend::compile_script(&CompilerOptions::default(), STAGE_GOTO_SOURCE, None);
    match compiled.result {
        Ok(output) => match verify_stage_goto_hook(&output.actions) {
            Ok(()) => return Ok(output.actions),
            Err(problem) => log::warn!(
                "Rascal's stage hook failed verification ({problem}); assembling it directly"
            ),
        },
        Err(diagnostics) => log::warn!(
            "Rascal could not compile the stage hook ({:?}); assembling it directly",
            diagnostics
        ),
    }
    let assembled = assemble_stage_goto_hook().map_err(|e| {
        vec![Diagnostic {
            severity: Severity::Error,
            message: format!("cannot assemble the stage hook: {e}"),
            origin: Origin::default(),
            line: 0,
            column: 0,
        }]
    })?;
    match verify_stage_goto_hook(&assembled) {
        Ok(()) => Ok(assembled),
        Err(problem) => Err(vec![Diagnostic {
            severity: Severity::Error,
            message: format!("the assembled stage hook is wrong: {problem}"),
            origin: Origin::default(),
            line: 0,
            column: 0,
        }]),
    }
}

/// A value on the simulated AVM1 stack.
#[derive(Debug, Clone, PartialEq)]
enum Sym {
    Str(String),
    Int(i32),
    Null,
    /// A variable (read with `GetVariable` or from a named register).
    Var(String),
    /// `object.name`.
    Member(Box<Sym>, String),
    /// The function defined by the instruction at this index.
    Function(usize),
    /// Anything else.
    Unknown,
}

/// A `CallMethod` seen by the simulation.
#[derive(Debug, Clone, PartialEq)]
struct MethodCall {
    object: Sym,
    method: String,
    args: Vec<Sym>,
}

/// What one block of code does, as far as the stage hook needs to know.
#[derive(Debug, Default)]
struct BlockEffects {
    /// Index (in the block) of the `Stop` action, if any.
    stop: Option<usize>,
    calls: Vec<(usize, MethodCall)>,
}

/// Runs the block that starts at `instructions[start]` (depth `depth`) on a
/// symbolic stack. Only the actions the stage hook uses are understood.
fn simulate(
    instructions: &[Instruction],
    start: usize,
    depth: usize,
    registers: &[(u8, String)],
) -> Result<BlockEffects, String> {
    let mut stack: Vec<Sym> = Vec::new();
    let mut effects = BlockEffects::default();
    let pop = |stack: &mut Vec<Sym>| stack.pop().unwrap_or(Sym::Unknown);
    for (index, instruction) in instructions.iter().enumerate().skip(start) {
        if instruction.depth < depth {
            break;
        }
        if instruction.depth > depth {
            continue;
        }
        match instruction.name.as_str() {
            "ConstantPool" => {}
            "Stop" => effects.stop = effects.stop.or(Some(index)),
            "Push" => stack.extend(instruction.operands.iter().map(|operand| {
                match operand {
                    Operand::Str(s) => Sym::Str(s.clone()),
                    Operand::Int(i) => Sym::Int(*i),
                    Operand::Number(n) if n.fract() == 0.0 && n.abs() < 1e9 => Sym::Int(*n as i32),
                    Operand::Null => Sym::Null,
                    Operand::Register(r) => registers
                        .iter()
                        .find(|(register, _)| register == r)
                        .map_or(Sym::Unknown, |(_, name)| Sym::Var(name.clone())),
                    _ => Sym::Unknown,
                }
            })),
            "GetVariable" => {
                let name = pop(&mut stack);
                stack.push(match name {
                    Sym::Str(name) => Sym::Var(name),
                    _ => Sym::Unknown,
                });
            }
            "GetMember" => {
                let name = pop(&mut stack);
                let object = pop(&mut stack);
                stack.push(match name {
                    Sym::Str(name) => Sym::Member(Box::new(object), name),
                    _ => Sym::Unknown,
                });
            }
            "CallMethod" => {
                let method = pop(&mut stack);
                let object = pop(&mut stack);
                let Sym::Int(count) = pop(&mut stack) else {
                    return Err("CallMethod without an argument count".to_owned());
                };
                let args = (0..count).map(|_| pop(&mut stack)).collect();
                let Sym::Str(method) = method else {
                    return Err("CallMethod without a method name".to_owned());
                };
                effects.calls.push((
                    index,
                    MethodCall {
                        object,
                        method,
                        args,
                    },
                ));
                stack.push(Sym::Unknown);
            }
            "DefineFunction" | "DefineFunction2" => stack.push(Sym::Function(index)),
            "Pop" => {
                pop(&mut stack);
            }
            "End" => break,
            other => return Err(format!("unexpected action {other}")),
        }
    }
    Ok(effects)
}

/// Checks what the stage hook bytecode does, by running it on a symbolic
/// stack: `stop()`, then `flash.external.ExternalInterface.addCallback(
/// "jz_goto", null, <function (f) { _root.gotoAndStop(f); }>)`.
pub(crate) fn verify_stage_goto_hook(actions: &[u8]) -> Result<(), String> {
    let instructions = disassemble(actions, 8).map_err(|e| e.to_string())?;
    if instructions.last().map(|i| (i.depth, i.name.as_str())) != Some((0, "End")) {
        return Err("the script does not end with ActionEnd".to_owned());
    }
    let top = simulate(&instructions, 0, 0, &[])?;
    let [(call_index, call)] = top.calls.as_slice() else {
        return Err(format!("expected one method call, found {:?}", top.calls));
    };
    if !top.stop.is_some_and(|stop| stop < *call_index) {
        return Err("stop() must come before the callback is registered".to_owned());
    }
    let interface = Sym::Member(
        Box::new(Sym::Member(
            Box::new(Sym::Var("flash".to_owned())),
            "external".to_owned(),
        )),
        "ExternalInterface".to_owned(),
    );
    if call.object != interface || call.method != "addCallback" {
        return Err(format!(
            "expected flash.external.ExternalInterface.addCallback, found {call:?}"
        ));
    }
    let [Sym::Str(name), Sym::Null, Sym::Function(function)] = call.args.as_slice() else {
        return Err(format!("unexpected addCallback arguments {:?}", call.args));
    };
    if name != "jz_goto" {
        return Err(format!("the callback is named {name:?}"));
    }
    let definition = &instructions[*function];
    if definition.operands != [Operand::Str(String::new()), Operand::Str("f".to_owned())] {
        return Err(format!("unexpected callback signature: {definition}"));
    }
    let body = simulate(&instructions, function + 1, 1, &definition.registers)?;
    let expected = MethodCall {
        object: Sym::Var("_root".to_owned()),
        method: "gotoAndStop".to_owned(),
        args: vec![Sym::Var("f".to_owned())],
    };
    if body.calls.iter().map(|(_, call)| call).ne([&expected]) {
        return Err(format!(
            "expected the callback to call _root.gotoAndStop(f), found {:?}",
            body.calls
        ));
    }
    Ok(())
}

/// The stage hook assembled action by action.
pub(crate) fn assemble_stage_goto_hook() -> std::io::Result<Vec<u8>> {
    let s = SwfStr::from_utf8_str;
    let mut body = Vec::new();
    write_actions(
        &mut body,
        &[
            push(vec![Value::ConstantPool(0)]), // "f"
            Action::GetVariable,
            push(vec![Value::Int(1), Value::ConstantPool(1)]), // "_root"
            Action::GetVariable,
            push(vec![Value::ConstantPool(2)]), // "gotoAndStop"
            Action::CallMethod,
            Action::Pop,
        ],
    )?;
    let mut out = Vec::new();
    write_actions(
        &mut out,
        &[
            Action::ConstantPool(ConstantPool {
                strings: [
                    "f",
                    "_root",
                    "gotoAndStop",
                    "jz_goto",
                    "flash",
                    "external",
                    "ExternalInterface",
                    "addCallback",
                ]
                .into_iter()
                .map(s)
                .collect(),
            }),
            Action::Stop,
            Action::DefineFunction(DefineFunction {
                name: s(""),
                params: vec![s("f")],
                actions: &body,
            }),
            push(vec![
                Value::Null,
                Value::ConstantPool(3), // "jz_goto"
                Value::Int(3),
                Value::ConstantPool(4), // "flash"
            ]),
            Action::GetVariable,
            push(vec![Value::ConstantPool(5)]), // "external"
            Action::GetMember,
            push(vec![Value::ConstantPool(6)]), // "ExternalInterface"
            Action::GetMember,
            push(vec![Value::ConstantPool(7)]), // "addCallback"
            Action::CallMethod,
            Action::Pop,
            Action::End,
        ],
    )?;
    Ok(out)
}

fn push(values: Vec<Value<'_>>) -> Action<'_> {
    Action::Push(Push { values })
}

fn write_actions(out: &mut Vec<u8>, actions: &[Action<'_>]) -> std::io::Result<()> {
    let mut writer = swf::avm1::write::Writer::new(out, 8);
    for action in actions {
        writer
            .write_action(action)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disasm::listing;

    #[test]
    fn rascal_compiles_the_hook_and_it_verifies() {
        let compiled =
            backend::compile_script(&CompilerOptions::default(), STAGE_GOTO_SOURCE, None);
        let actions = compiled
            .result
            .expect("Rascal compiles the stage hook")
            .actions;
        let instructions = disassemble(&actions, 8).expect("valid bytecode");
        assert_eq!(
            verify_stage_goto_hook(&actions),
            Ok(()),
            "{}",
            listing(&instructions)
        );
    }

    #[test]
    fn public_hook_is_the_verified_rascal_output() {
        let hook = stage_goto_hook().expect("stage hook");
        assert_eq!(verify_stage_goto_hook(&hook), Ok(()));
        let compiled =
            backend::compile_script(&CompilerOptions::default(), STAGE_GOTO_SOURCE, None);
        assert_eq!(hook, compiled.result.expect("compiles").actions);
    }

    #[test]
    fn assembled_fallback_verifies_too() {
        let assembled = assemble_stage_goto_hook().expect("assembles");
        let instructions = disassemble(&assembled, 8).expect("valid bytecode");
        assert_eq!(
            verify_stage_goto_hook(&assembled),
            Ok(()),
            "{}",
            listing(&instructions)
        );
    }

    #[test]
    fn verifier_rejects_a_bare_identifier_call() {
        // `ExternalInterface.addCallback(...)` without the package reads the
        // wrong variable.
        let compiled = backend::compile_script(
            &CompilerOptions::default(),
            "stop();\nExternalInterface.addCallback(\"jz_goto\", null, function (f) { _root.gotoAndStop(f); });",
            None,
        );
        let actions = compiled.result.expect("compiles").actions;
        assert!(verify_stage_goto_hook(&actions).is_err());
    }
}
