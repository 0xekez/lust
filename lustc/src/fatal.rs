use crate::{
    compiler::{self, Context, JIT},
    conversions,
    data::LustData,
    foreign, Expr,
};
use cranelift::prelude::*;

impl Expr {
    // Determines if an expression is an error expression and returns
    // its messsage and return code arguments.
    pub fn is_error(&self) -> Option<(&Expr, &Expr)> {
        if let Expr::List(v) = self {
            if let Some(Expr::Symbol(s)) = v.first() {
                if s == "error" && v.len() == 3 {
                    return Some((&v[1], &v[2]));
                }
            }
        }
        None
    }
}

pub(crate) fn emit_error_strings(jit: &mut JIT) -> Result<(), String> {
    let error_strings = [(
        "__anon_data_bad_call_type",
        "fatal error: non-closure object in head position of list",
    )];
    let error_data = error_strings
        .iter()
        .map(|(name, msg)| -> Result<LustData, std::ffi::NulError> {
            Ok(LustData {
                name: name.to_string(),
                data: crate::conversions::string_to_immediate(&std::ffi::CString::new(*msg)?),
            })
        })
        .collect::<Result<Vec<LustData>, _>>()
        .map_err(|e| e.to_string())?;

    error_data
        .into_iter()
        .map(|d| crate::data::create_data(d, jit))
        .collect()
}

pub(crate) fn emit_error(
    message: &Expr,
    exit_code: &Expr,
    ctx: &mut Context,
) -> Result<Value, String> {
    foreign::emit_foreign_call("puts", &[message.clone()], ctx)?;
    foreign::emit_foreign_call("exit", &[exit_code.clone()], ctx)
}

pub(crate) fn emit_check_callable(query: &Expr, ctx: &mut Context) -> Result<(), String> {
    let accum = compiler::emit_expr(query, ctx)?;
    let accum = ctx
        .builder
        .ins()
        .band_imm(accum, conversions::HEAP_TAG_MASK);
    let cond = ctx
        .builder
        .ins()
        .icmp_imm(IntCC::Equal, accum, conversions::CLOSURE_TAG);

    let error_block = ctx.builder.create_block();
    let ok_block = ctx.builder.create_block();

    ctx.builder.ins().brz(cond, error_block, &[]);
    ctx.builder.ins().jump(ok_block, &[]);

    ctx.builder.switch_to_block(error_block);
    ctx.builder.seal_block(error_block);

    emit_error(
        &Expr::Symbol("__anon_data_bad_call_type".to_string()),
        &Expr::Integer(-1),
        ctx,
    )?;

    // This ought to be unreachable but it appeases the code
    // generator.
    ctx.builder.ins().jump(ok_block, &[]);

    ctx.builder.switch_to_block(ok_block);
    ctx.builder.seal_block(ok_block);

    Ok(())
}
