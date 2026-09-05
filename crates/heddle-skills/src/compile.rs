//! Recipe to [`Workflow`]: the whole of this crate's job.
//!
//! Three things happen here, in this order, and the order is the design
//! (`plan.md` D3): every required extension is checked, then every parameter is
//! resolved, then one [`Node`] is emitted per [`RecipeStep`]. Validation
//! precedes construction so that a recipe which cannot run is refused with
//! nothing built and nothing to undo — as opposed to a `Node::Tool` naming an
//! absent tool, which would fail deep inside the gateway, mid-run, after
//! whatever preceded it had already taken effect.

use crate::error::{Result, SkillError};
use crate::recipe::{Recipe, RecipeStep};
use heddle_core::{Message, ToolCall};
use heddle_workflow::{Node, Workflow};
use serde_json::Value;
use std::collections::HashMap;

/// Compiles a recipe into the workflow the native engine executes.
///
/// `available_tools` is what the *target* gateway advertises — passed in rather
/// than discovered, because this crate holds no gateway and must name no
/// connector (Constitution IV). The caller that owns a
/// [`ToolGateway`](heddle_core::ToolGateway) has already got the answer from
/// `advertise()`, policy-filtered, which is the list that actually matters: a
/// tool the gateway knows but the policy forbids is not available to this run.
///
/// Parameters not declared by the recipe are ignored rather than refused. A
/// caller passing a map that serves several recipes is doing something
/// reasonable, and a typo in a caller's key still surfaces — as the
/// [`SkillError::MissingParam`] for the declared parameter it failed to supply.
pub fn compile(
    recipe: &Recipe,
    params: &HashMap<String, String>,
    available_tools: &[&str],
) -> Result<Workflow> {
    // 1. Refuse before building. See this module's header and `plan.md` D3.
    for extension in &recipe.required_extensions {
        if !available_tools.contains(&extension.as_str()) {
            return Err(SkillError::MissingExtension {
                recipe: recipe.name.clone(),
                extension: extension.clone(),
                available: if available_tools.is_empty() {
                    "none".to_string()
                } else {
                    available_tools.join(", ")
                },
            });
        }
    }

    // 2. Resolve every declared parameter to exactly one value: the caller's if
    //    it supplied one, otherwise the declared default. A parameter with
    //    neither is not resolvable, but it is only an *error* if something
    //    actually references it — which `substitute` decides, so that a recipe
    //    carrying an unused optional parameter is not refused for it.
    let mut resolved: HashMap<&str, &str> = HashMap::new();
    for param in &recipe.params {
        let value = params
            .get(&param.name)
            .map(String::as_str)
            .or(param.default.as_deref());
        if let Some(value) = value {
            resolved.insert(param.name.as_str(), value);
        }
    }

    // 3. One node per step, in order, with the recipe's persona folded into
    //    every agent prompt.
    let persona = substitute(&recipe.prompt, &resolved, recipe, "the recipe prompt")?;
    let mut graph = Vec::with_capacity(recipe.instructions.len());
    for step in &recipe.instructions {
        let context = || format!("step {}", step.id());
        graph.push(match step {
            RecipeStep::Agent { id, prompt } => Node::Agent {
                id: id.clone(),
                prompt: Message::user_text(join_persona(
                    &persona,
                    &substitute(prompt, &resolved, recipe, &context())?,
                )),
            },
            RecipeStep::Tool { id, tool, args } => Node::Tool {
                id: id.clone(),
                // `ToolCall::new` and not `with_id`: the id a provider supplies
                // answers a model's own request, and nothing here is answering
                // one — a compiled tool step is a call the *recipe* makes.
                call: ToolCall::new(
                    tool.clone(),
                    substitute_json(args, &resolved, recipe, &context())?,
                ),
            },
            RecipeStep::Approval { id, message } => Node::Approval {
                id: id.clone(),
                message: substitute(message, &resolved, recipe, &context())?,
            },
        });
    }

    Ok(Workflow::new(recipe.name.clone(), graph))
}

/// Joins the recipe-level persona to a step's own prompt.
///
/// A blank line between them, and nothing at all when there is no persona: a
/// message that opens with two newlines is a message whose first instruction to
/// the model is whitespace.
fn join_persona(persona: &str, prompt: &str) -> String {
    if persona.is_empty() {
        prompt.to_string()
    } else {
        format!("{persona}\n\n{prompt}")
    }
}

/// Replaces every `{{name}}` with its resolved value.
///
/// A single forward scan rather than a `str::replace` per parameter, and the
/// reason is the error case rather than efficiency: `replace` cannot tell a
/// placeholder naming no declared parameter from one it has already
/// substituted, so a typo would survive into a prompt as literal braces. This
/// scan sees every placeholder the author wrote and can refuse the ones it
/// cannot resolve (`plan.md` D4).
///
/// It is deliberately not a template engine — no conditionals, no loops, no
/// filters, no nesting. `{{name}}` is the entire grammar (Constitution VII).
fn substitute(
    template: &str,
    resolved: &HashMap<&str, &str>,
    recipe: &Recipe,
    context: &str,
) -> Result<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        let close = after_open
            .find("}}")
            .ok_or_else(|| SkillError::UnterminatedPlaceholder {
                recipe: recipe.name.clone(),
                context: context.to_string(),
            })?;
        let name = after_open[..close].trim();
        let value = resolved.get(name).ok_or_else(|| SkillError::MissingParam {
            recipe: recipe.name.clone(),
            param: name.to_string(),
        })?;
        out.push_str(value);
        rest = &after_open[close + 2..];
    }
    out.push_str(rest);

    Ok(out)
}

/// Substitutes the string leaves of a tool step's arguments, at any depth.
///
/// Strings only, because a placeholder is textual and a number or a boolean has
/// no braces to expand. Keys are left alone: a recipe that parameterized an
/// argument's *name* would be reshaping the tool's schema rather than filling
/// it in, which is a different thing from what `{{param}}` promises.
fn substitute_json(
    args: &Value,
    resolved: &HashMap<&str, &str>,
    recipe: &Recipe,
    context: &str,
) -> Result<Value> {
    Ok(match args {
        Value::String(s) => Value::String(substitute(s, resolved, recipe, context)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| substitute_json(v, resolved, recipe, context))
                .collect::<Result<Vec<_>>>()?,
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), substitute_json(v, resolved, recipe, context)?)))
                .collect::<Result<serde_json::Map<_, _>>>()?,
        ),
        other => other.clone(),
    })
}
