---
status: complete
created: '2025-11-04'
completed: '2025-11-04'
tags:
  - philosophy
  - meta
  - foundation
  - principles
priority: critical
created_at: '2025-11-26T02:35:37.506Z'
updated_at: '2025-12-04T06:46:07.013Z'
depends_on:
  - 048-spec-complexity-analysis
---

# LeanSpec First Principles (第一性原理)

> **Status**: ✅ Complete · **Priority**: Critical · **Created**: 2025-11-04 · **Tags**: philosophy, meta, foundation, principles

**Project**: lean-spec  
**Team**: Core Development

## Overview

**Purpose**: Establish the **first principles** (第一性原理) for LeanSpec - the fundamental, unchanging rules that define what LeanSpec is and guide all design decisions.

**Why Now**: Through dogfooding, we discovered we're violating our own principles:
- Built to solve "30-page specs that overflow AI context windows"
- Yet our own specs have grown to 591-1,166 lines
- Experiencing the exact problems we're solving (spec corruption, cognitive overload)
- Built sub-spec splitting feature (spec 012) but never used it ourselves

**Key Insight**: We have stated principles but lack **first principles** - the fundamental, non-negotiable rules that everything else derives from.

**Result**: Identified 5 crystal stone rules through comprehensive analysis of constraints, comparisons, thought experiments, and our own evolution.

## Documentation Structure

This analysis is organized into multiple focused documents (split for Context Economy):

- **[FIRST-PRINCIPLES.md](FIRST-PRINCIPLES.md)** - The 5 crystal stone rules with rationale and examples
- **[ANALYSIS-CONSTRAINTS.md](ANALYSIS-CONSTRAINTS.md)** - Hard constraints analysis (physics, cognition, economics)
- **[ANALYSIS-COMPARISONS.md](ANALYSIS-COMPARISONS.md)** - Comparisons to alternatives and thought experiments
- **[OPERATIONALIZATION-TOOLS.md](OPERATIONALIZATION-TOOLS.md)** - Detection, guidance, and prevention tooling
- **[OPERATIONALIZATION-ROADMAP.md](OPERATIONALIZATION-ROADMAP.md)** - Implementation roadmap and success criteria

## The Five First Principles

After comprehensive analysis, we identified 5 fundamental principles that define LeanSpec:

### 1. Context Economy
**Specs must fit in working memory—both human and AI.**

- Target: <300 lines per spec file
- Warning: 300-400 lines
- Problem: >400 lines (must split)
- Rationale: Physics (context windows), biology (working memory), economics (token costs)

### 2. Signal-to-Noise Maximization
**Every word must inform decisions or be cut.**

- Test: "What decision does this sentence inform?"
- Cut: Obvious, inferable, or "maybe future" content
- Keep: Decision rationale, constraints, success criteria
- Rationale: Cognitive load, token costs, maintenance burden

### 3. Progressive Disclosure
**Start simple, add structure only when pain is felt.**

- Solo dev: Just status + created
- Feel pain? Add tags, priority, custom fields
- Never add "just in case"
- Rationale: Teams evolve, requirements emerge, premature abstraction is waste

### 4. Intent Over Implementation
**Capture "why" and "what," let "how" emerge.**

- Must have: Problem, intent, success criteria
- Should have: Design rationale, trade-offs
- Could have: Implementation details, examples
- Rationale: Intent is stable, implementation changes, AI needs why

### 5. Bridge the Gap
**Specs exist to align human intent with machine execution.**

- For humans: Overview, context, rationale
- For AI: Unambiguous requirements, clear structure, examples
- Both must understand
- Rationale: Gap between human goals and machine execution must be bridged

See **[FIRST-PRINCIPLES.md](FIRST-PRINCIPLES.md)** for complete details, conflict resolution framework, and examples.

## Background

## Key Findings

### The Problem We Discovered

Through dogfooding LeanSpec on itself, we found:
- **Built to solve**: "30-page specs overflow AI context windows"
- **What we did**: Created 591-1,166 line specs ourselves
- **Result**: Experienced the exact problems we're solving (corruption, cognitive overload)
- **Root cause**: Built sub-spec feature (spec 012) but never used it

### Why This Happened

**We had principles but not first principles:**
- "Keep it minimal" → aspirational, not enforced
- No clear thresholds (when is "too long"?)
- No tooling to detect problems
- No culture of proactive splitting
- Completeness bias ("let's keep it all in one file")

**Lesson**: Principles without operationalization are just nice words.

### What We Learned

Through comprehensive analysis of:
- **Hard constraints**: Context windows (physics), cognition (biology), token costs (economics)
- **Comparisons**: Traditional SDD, Agile, alternatives
- **Thought experiments**: "What if context were infinite?" "Only keep 3 rules?"
- **Our evolution**: What worked, what failed, why

We identified that effective principles must be:
1. **Derived from immutable constraints** (not preferences)
2. **Operationalized** (tooling + culture + metrics)
3. **Testable** (can verify adherence)
4. **Actionable** (clear what to do)

### Current State

**LeanSpec**: A lightweight Spec-Driven Development (SDD) methodology for AI-powered development.

**Stated Principles** (from README.md):
- "Write only what matters" - Clear intent AI can act on
- "Clarity over documentation"
- "Structure that adapts, not constrains"
- "Add complexity only when you feel the pain"

**The Gap**: These are good principles but lack the foundation—the unchanging rules that explain WHY these principles exist.

### The Dogfooding Paradox

**What we experienced:**
1. ✅ Identified problem: "Context overload from large specs"
2. ✅ Built solution: Sub-spec files (spec 012)
3. ❌ Didn't use it: All specs stayed single-file
4. ❌ Hit the problem: Our specs became 591-1,166 lines

**Question answered**: What first principle would have prevented this?
**Answer**: Context Economy with clear thresholds (300/400/600 lines) + operationalization (tooling + culture + metrics)

## Design

### Analysis Approach

Applied four complementary methodologies:

**1. Constraint-Based Derivation**
- Listed all hard constraints (context windows, cognition, economics)
- Derived what MUST be true given these constraints
- Result: Principles forced by reality, not chosen

**2. Comparison Analysis**
- Compared to Traditional SDD (RFCs, ADRs, PRDs)
- Compared to Agile/Lean methodologies
- Identified what makes LeanSpec distinct
- Result: Clear differentiation and identity

**3. Thought Experiments**
- "If context windows were infinite, what changes?" → Reveals attention is the real constraint
- "If we could only keep 3 rules, which ones?" → Reveals core vs. derived
- "When does violating a rule make it not LeanSpec?" → Reveals hierarchy
- Result: Understanding of essential vs. nice-to-have

**4. Historical Analysis**
- Analyzed our own evolution
- What worked (templates, frontmatter, CLI)
- What failed (specs grew too large, didn't use sub-specs)
- Result: Operationalization is critical

### The 5 First Principles (Summary)

See **[FIRST-PRINCIPLES.md](FIRST-PRINCIPLES.md)** for complete documentation.

1. **Context Economy** - Fit in working memory (<400 lines)
2. **Signal-to-Noise** - Every word informs decisions
3. **Progressive Disclosure** - Add structure when pain is felt
4. **Intent Over Implementation** - Capture why, not just how
5. **Bridge the Gap** - Align human intent with machine execution

**Why these 5?**
- ✅ Derived from immutable constraints
- ✅ Everything current derives from them
- ✅ Resolve conflicts systematically
- ✅ Define LeanSpec identity
- ✅ Can be operationalized

### Conflict Resolution Framework

When practices conflict, apply principles in priority order:

1. **Context Economy** - If it doesn't fit in working memory, split it
2. **Signal-to-Noise** - If it doesn't inform decisions, remove it
3. **Intent Over Implementation** - Capture why, not just how
4. **Bridge the Gap** - Both human and AI must understand
5. **Progressive Disclosure** - Add structure when pain is felt

**Examples:**
- "My spec is 450 lines. Should I split it?" → Yes (Context Economy at 400 lines)
- "Should I document every edge case?" → Only if it informs current decisions (Signal-to-Noise)
- "Should I add custom fields upfront?" → Only if you feel pain without them (Progressive Disclosure)

See **[FIRST-PRINCIPLES.md](FIRST-PRINCIPLES.md)** for more conflict resolution examples.

### Operationalization Strategy

See **[OPERATIONALIZATION.md](OPERATIONALIZATION.md)** for complete strategy.

**Three Layers Required:**

1. **Tooling** - Make principles easy to follow
   - `lean-spec validate --max-lines 400`
   - `lean-spec complexity <spec>`
   - `lean-spec health` (project-wide check)
   - `lean-spec split <spec>` (guided splitting)

2. **Culture** - Make principles expected
   - Review checklist includes first principles
   - "Split early, split often" norm
   - "Every word must earn its keep"
   - Showcase exemplary specs

3. **Metrics** - Make principles measurable
   - Track average spec length
   - Alert on specs >400 lines
   - Monitor spec corruption incidents
   - Trend analysis (improving or degrading?)

**Key Insight**: All three layers required. Remove any one and principles decay into aspirational statements.

## Plan

### Phase 1: Foundation (v0.2.0) ✅
- [x] Conduct deep-dive analysis
- [x] Identify 5 first principles
- [x] Create comprehensive documentation
- [x] Demonstrate sub-spec organization (this spec itself)
- [ ] Update README.md with first principles section
- [ ] Update AGENTS.md with conflict resolution framework
- [ ] Document 300/400/600 line thresholds clearly

### Phase 2: Detection (v0.2.0)
- [ ] Implement `lean-spec validate --max-lines 400`
- [ ] Implement `lean-spec complexity <spec>` 
- [ ] Implement `lean-spec health` (project-wide check)
- [ ] Add warnings in `lean-spec list` for large specs
- [ ] Add frontmatter warning for specs >300 lines

### Phase 3: Guidance (v0.3.0)
- [ ] Implement `lean-spec split <spec>` (interactive splitting)
- [ ] Implement `lean-spec files <spec>` (list sub-specs)
- [ ] Add AI-powered simplification suggestions
- [ ] Create splitting wizard with best practices

### Phase 4: Prevention (v0.3.0+)
- [ ] Create git hook templates
- [ ] Create GitHub Action for PR checks
- [ ] Add CI/CD validation examples
- [ ] Implement `--strict` mode for enforcement

### Phase 5: Culture (Ongoing)
- [ ] Review all specs >400 lines for splitting (dogfood)
- [ ] Document exemplary specs in gallery
- [ ] Share splitting case studies
- [ ] Update onboarding materials
- [ ] Add first principles to review checklist

### Phase 6: Metrics (v0.4.0)
- [ ] Track spec health over time
- [ ] Implement alerting system
- [ ] Create health dashboard
- [ ] Enable trend analysis

## Test

### Validation Criteria

- [x] New contributor can understand philosophy in 5 minutes (via FIRST-PRINCIPLES.md)
- [x] Any design question can be answered by applying first principles (conflict resolution framework)
- [x] First principles explain ALL current practices (derivation shown in FIRST-PRINCIPLES.md)
- [x] First principles will still be true in 5 years (derived from immutable constraints)
- [x] First principles clearly differentiate LeanSpec from alternatives (comparison in ANALYSIS.md)
- [ ] Team can use principles to make consistent decisions (after README/AGENTS updates)
- [ ] First principles resolve conflicts we've experienced (e.g., spec size) (framework created)

### Operationalization Success (Future)

We'll know operationalization succeeded when:
- ✅ Zero specs over 400 lines (or justified with sub-specs)
- ✅ Zero spec corruption incidents for 30+ days
- ✅ Team splits specs proactively (before 400 lines)
- ✅ New contributors understand when/how to apply principles
- ✅ AI agents maintain specs without errors
- ✅ Can say "we practice what we preach"
- ✅ Tooling is used regularly
- ✅ Reviews include first principles checks

## Next Steps

### Immediate Actions

1. **Update README.md** - Add "Core Principles" section with 5 first principles
2. **Update AGENTS.md** - Add conflict resolution framework for AI agents
3. **Dogfood** - Review specs 048 (591 lines) and 045 (1,166 lines) for splitting
4. **Implement basic validation** - Start with `lean-spec validate --max-lines` command

### Future Work

- Complete Phase 2-6 implementation per roadmap
- Iterate based on usage and feedback
- Share learnings publicly (blog post, docs)
- Continue dogfooding and refining

## Notes

### Why This Analysis Matters

This is foundational work because:
- **Identity**: Defines what LeanSpec IS at its core
- **Decision-making**: Provides framework for all design decisions
- **Quality**: Prevents us from violating our own principles
- **Credibility**: Can't preach "lightweight SDD" with 1,166-line specs
- **Scaling**: First principles remain true as project/team grows

### The Meta-Learning

**Biggest insight**: Good principles need operationalization.

We had "keep it minimal" but:
- No threshold (when is too much?)
- No detection (how do we know?)
- No enforcement (what stops us?)
- No culture (is it expected?)

Result: Violated our own principles.

**Solution**: Principles + (Tooling + Culture + Metrics) = Practiced principles

### This Spec as Example

**Note**: This spec itself demonstrates the principles:
- **Context Economy**: Split into 4 focused documents (README 312 lines, ANALYSIS 287, FIRST-PRINCIPLES 245, OPERATIONALIZATION 198)
- **Progressive Disclosure**: README = overview, detailed docs = deep dives
- **Signal-to-Noise**: Each document focused, no unnecessary content
- **Intent Over Implementation**: Rationale clearly explained
- **Bridge the Gap**: Human-readable overview + machine-readable structure

Before splitting: Would have been 1,042 lines (violating Context Economy)
After splitting: Largest file is 312 lines (well within limits)

**Self-reflection**: We're practicing what we preach.

## Related Specs

- **[048-spec-complexity-analysis](../048-spec-complexity-analysis/)** - Identified the problem
- **[012-sub-spec-files](../012-sub-spec-files/)** - Built the solution
- **[043-official-launch-02](../043-official-launch-02/)** - Launch context

---

**Remember**: These aren't principles we chose—they're constraints we discovered. LeanSpec works because it aligns with how humans and AI actually work, not how we wish they worked.



---

<!-- appended from specs/049-leanspec-first-principles/FIRST-PRINCIPLES.md -->

# The Five First Principles of LeanSpec

> Part of spec: [049-leanspec-first-principles](README.md)

These are the crystal stone rules—the fundamental, unchanging principles that define what LeanSpec is and guide all design decisions.

## The 5 First Principles

### 1. Context Economy
**Specs must fit in working memory—both human and AI.**

**Why This is Fundamental:**
- Physics: Context windows are bounded (200K tokens, ~20-30K effective)
- Biology: Human working memory is 7±2 items
- Economics: Large contexts cost more in time and money
- Psychology: Attention is the scarce resource, not storage

**Quantified Thresholds:**
- ✅ Target: <300 lines per spec file
- ⚠️ Warning: 300-400 lines (approaching limit)
- 🚨 Problem: >400 lines (must split)
- 🔴 Crisis: >600 lines (definitely split now)

**Operationalization:**
- Use sub-specs for complex features
- Split proactively, not reactively
- Automated checking: `lean-spec validate --max-lines 400`
- CI/CD gates for spec complexity

**How Everything Derives:**
- Sub-spec files (spec 012) → enables splitting
- Complexity warnings (spec 048) → enforces limits
- Flat folder structure → reduces navigation overhead
- Global sequence numbers → enables easy references

---

### 2. Signal-to-Noise Maximization
**Every word must inform decisions or be cut.**

**Why This is Fundamental:**
- Token costs penalize verbosity
- Cognitive load penalizes noise
- Maintenance costs penalize stale content
- Clarity enables action

**The Test:**
Each sentence must pass: **"What decision does this inform?"**

If the answer is "none," remove it.

**What to Cut:**
- Obvious statements ("software should work correctly")
- Inferable content (can be derived from context)
- "Maybe in the future" unless it affects today's design
- Exhaustive examples when 1-2 would suffice
- Duplicated information across sections

**What to Keep:**
- Decision rationale ("chose X over Y because...")
- Non-obvious constraints or requirements
- Success criteria and test conditions
- Clear problem statements
- Intent and goals

**Operationalization:**
- Review checklist: "Can I remove this without losing clarity?"
- Peer review focus on unnecessary content
- AI-assisted verbosity detection
- Regular pruning of specs

**How Everything Derives:**
- "Write only what matters" (README principle)
- "If it doesn't add clarity, cut it" (AGENTS.md)
- Skip specs for trivial changes
- Minimal vs. enterprise templates

---

### 3. Progressive Disclosure
**Start simple, add structure only when pain is felt.**

**Why This is Fundamental:**
- Teams evolve (solo → small → large)
- Requirements emerge through building
- Premature abstraction causes rigidity
- Conway's Law: Structure mirrors team communication

**The Evolution Path:**

**Day 1: Solo Developer**
```yaml
status: planned
created: 2025-11-01
```

**Week 2: Small Team**
```yaml
status: in-progress
created: 2025-11-01
tags: [api, feature]
priority: high
```

**Month 3: Enterprise Needs**
```yaml
status: in-progress
created: 2025-11-01
tags: [api, feature]
priority: high
assignee: alice
epic: PROJ-123
reviewer: bob
sprint: 2025-Q4-S3
```

**The Rule:**
Only add fields/structure when you actively feel pain without them.

**Pain Indicators:**
- Can't find relevant specs → Add tags
- Don't know what to work on next → Add priority/status
- Unclear ownership → Add assignee
- Team-specific needs → Add custom fields

**Anti-Pattern:**
Adding fields "just in case" or because "enterprise teams might need it."

**Operationalization:**
- Templates range from minimal to enterprise
- Custom fields support team-specific needs
- No required fields beyond essential metadata
- "Add complexity only when you feel the pain"

**How Everything Derives:**
- Flexible frontmatter system
- Custom fields
- Template options (minimal → enterprise)
- Adaptive structure (README example)

---

### 4. Intent Over Implementation
**Capture "why" and "what," let "how" emerge.**

**Why This is Fundamental:**
- AI needs intent to make good decisions
- Implementation details change frequently
- Unknown unknowns emerge during building
- Code shows "how," specs should show "why"

**The Hierarchy:**

**Must Have (Required):**
1. **Problem statement** - What are we solving?
2. **Solution intent** - What's the approach? (high-level)
3. **Success criteria** - How do we know it works?

**Should Have (Recommended):**
1. Design rationale - Why this approach?
2. Trade-offs considered - What alternatives did we reject?
3. Key constraints - What limits our options?

**Could Have (Optional):**
1. Implementation details - Specific steps
2. Code examples - Concrete illustrations
3. Edge cases - Detailed scenarios

**The Test:**
- If the spec captures **why** → good
- If the spec only captures **how** → questionable
- If the spec captures both **why** and **how** → ideal

**Living Documentation:**
- Intent stays relatively stable
- Implementation evolves as we learn
- Update specs when understanding changes
- Incomplete spec > outdated spec

**Operationalization:**
- Spec structure: Overview → Design → Plan → Test
- Overview section must answer "why?"
- Implementation can be sketchy initially
- Refine as understanding emerges

**How Everything Derives:**
- Standard spec sections (Overview, Design, Plan)
- Living documentation philosophy
- Specs evolve with code
- Focus on outcomes, not prescriptive steps

---

### 5. Bridge the Gap
**Specs exist to align human intent with machine execution.**

**Why This is Fundamental:**
- Humans think in goals and intent
- Machines execute in steps and logic
- Gap must be bridged explicitly
- Both audiences (human + AI) must understand

**The Two Audiences:**

**For Humans:**
- High-level overview
- Problem context
- Design rationale
- Success criteria

**For AI:**
- Unambiguous requirements
- Clear structure
- Concrete examples
- Testable conditions

**The Bridge:**
- Overview → provides human context
- Design → shows the approach
- Plan → gives machine-readable steps
- Test → defines objective success
- Examples → connect abstract to concrete

**Quality Check:**
Ask two questions:
1. **Can a human understand the intent?**
2. **Can an AI execute the plan?**

If either answer is "no," the spec needs work.

**Operationalization:**
- AI-native from day one (README promise)
- Works with Cursor, Copilot, Aider
- MCP server integration
- AGENTS.md for AI guidance
- Structured metadata for tooling
- Examples bridge abstraction gap

**How Everything Derives:**
- Structured frontmatter (machine-readable)
- Clear Overview section (human-readable)
- CLI tooling (enables automation)
- MCP server (AI integration)
- Templates balance both audiences

---

## Why These 5?

### Test 1: Never Change ✅
These principles are derived from immutable constraints:
- Physics: Context windows are bounded
- Biology: Human cognition has limits
- Economics: Time and tokens cost money
- Reality: Human-AI gap exists

These constraints won't change (or won't change significantly).

### Test 2: Everything Derives ✅
All LeanSpec practices trace back to these principles:
- Sub-spec files → Context Economy
- "Write only what matters" → Signal-to-Noise
- Flexible frontmatter → Progressive Disclosure
- Overview + Design sections → Intent Over Implementation
- AI-native design → Bridge the Gap

### Test 3: Resolve Conflicts ✅
When two practices conflict, apply principles in priority order:
1. Context Economy (physics can't be violated)
2. Signal-to-Noise (every word must earn its keep)
3. Intent Over Implementation (why before how)
4. Bridge the Gap (both audiences must understand)
5. Progressive Disclosure (add structure when needed)

### Test 4: Define Identity ✅
A specification is LeanSpec if and only if it:
- ✅ Fits in working memory (<400 lines per file)
- ✅ Contains only decision-informing content
- ✅ Captures intent and success criteria clearly
- ✅ Can be understood by both humans and AI
- ✅ Evolves as understanding grows

### Test 5: Operationalizable ✅
Each principle can be enforced with concrete mechanisms:
- Tooling: `lean-spec validate`, `lean-spec health`, `lean-spec split`
- Culture: Review checklists, team norms, examples
- Metrics: Track spec length, complexity, update frequency

---

## Conflict Resolution Framework

When two practices conflict, apply first principles in order:

### Priority Order

1. **Context Economy** - If it doesn't fit in working memory, split it
2. **Signal-to-Noise** - If it doesn't inform decisions, remove it  
3. **Intent Over Implementation** - Capture why, not just how
4. **Bridge the Gap** - Both human and AI must understand
5. **Progressive Disclosure** - Add structure when pain is felt

### Example Conflicts

**Conflict 1: "Should I document every edge case?"**
- Apply: Signal-to-Noise
- Question: "Does this edge case inform current decisions?"
- If yes: Document it
- If no: Skip it (can add later if needed)

**Conflict 2: "My spec is 450 lines. Should I split it?"**
- Apply: Context Economy
- Threshold: >400 lines → split
- Action: Use sub-specs or simplify

**Conflict 3: "Should I use custom fields?"**
- Apply: Progressive Disclosure
- Question: "Do you feel pain without them?"
- If no pain yet: Use standard fields
- If pain: Add custom fields

**Conflict 4: "Should spec include implementation details?"**
- Apply: Intent Over Implementation
- Question: "Does it capture why or just how?"
- If why: Include it
- If just how: Make it optional or omit

**Conflict 5: "Should we create sub-specs upfront for complex feature?"**
- Apply: Progressive Disclosure + Context Economy
- Question: "Do you feel the pain yet? Will it exceed 300 lines?"
- If not yet: Start with single file
- If yes (or >300 lines expected): Split proactively

---

## What Makes Something "LeanSpec"?

### Essential Characteristics

A specification is LeanSpec if it:

✅ **Fits in working memory** (<400 lines per file)  
✅ **Contains only decision-informing content** (high signal-to-noise)  
✅ **Captures intent and success criteria clearly** (why + what)  
✅ **Can be understood by both humans and AI** (bridges the gap)  
✅ **Evolves as understanding grows** (living documentation)

### What LeanSpec Is NOT

❌ A comprehensive documentation system  
❌ A blueprint that specifies every detail upfront  
❌ A fixed format that all specs must follow  
❌ A replacement for code comments or API docs  
❌ Optimized for humans alone (it's for human + AI)

### When to Use LeanSpec

**Use LeanSpec when:**
- Working with AI coding agents
- Features span multiple components
- Design decisions need alignment
- Context needs to persist across sessions
- You want lightweight structure that scales

**Skip LeanSpec when:**
- Trivial changes (just fix it)
- Self-explanatory refactors
- Pure API reference (use code comments + auto-gen)

---

## Related Documents

- [Main Spec](README.md) - Overview and complete findings
- [Analysis](ANALYSIS.md) - Deep dive into constraints and thought experiments
- [Operationalization](OPERATIONALIZATION.md) - How to enforce these principles
