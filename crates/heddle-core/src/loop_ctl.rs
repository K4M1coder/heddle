//! Loop engineering (design §4.14, Constitution VIII): engine-enforced loop
//! control. The model never decides when to stop; the budget does, and
//! reflect/retry is anchored to external ground truth (not self-judgment).

/// Externally-enforced budget. Any limit hitting zero-remaining ends the loop.
#[derive(Debug, Clone)]
pub struct LoopBudget {
    pub max_iters: u32,
    pub max_tokens: u64,
    pub no_progress_limit: u32,
}

impl LoopBudget {
    pub fn new(max_iters: u32, max_tokens: u64, no_progress_limit: u32) -> Self {
        LoopBudget {
            max_iters,
            max_tokens,
            no_progress_limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// A terminal state was produced by the agent (allowed stop).
    FinalOutput,
    MaxIters,
    MaxTokens,
    NoProgress,
    HumanReject,
}

/// Tracks a single loop's spend and decides, each iteration, whether to stop.
/// Ground truth in: `record_iteration(tokens_used, made_progress)`.
pub struct LoopController {
    budget: LoopBudget,
    iters: u32,
    tokens: u64,
    stale: u32,
}

impl LoopController {
    pub fn new(budget: LoopBudget) -> Self {
        LoopController {
            budget,
            iters: 0,
            tokens: 0,
            stale: 0,
        }
    }

    pub fn iters(&self) -> u32 {
        self.iters
    }
    pub fn tokens(&self) -> u64 {
        self.tokens
    }

    /// Call once per completed iteration with observed cost and a ground-truth
    /// progress signal (e.g. tests newly passing, a tool result advancing state).
    /// `made_progress` MUST come from outside the model, never from self-report.
    pub fn record_iteration(&mut self, tokens_used: u64, made_progress: bool) {
        self.iters += 1;
        self.tokens += tokens_used;
        if made_progress {
            self.stale = 0;
        } else {
            self.stale += 1;
        }
    }

    /// The engine's stop decision. Returns Some(Exit) when a budget is exhausted.
    /// `final_output` is the only model-driven stop, and it is still checked here.
    pub fn should_exit(&self, final_output: bool) -> Option<Exit> {
        if final_output {
            return Some(Exit::FinalOutput);
        }
        if self.iters >= self.budget.max_iters {
            return Some(Exit::MaxIters);
        }
        if self.tokens >= self.budget.max_tokens {
            return Some(Exit::MaxTokens);
        }
        if self.stale >= self.budget.no_progress_limit {
            return Some(Exit::NoProgress);
        }
        None
    }
}
