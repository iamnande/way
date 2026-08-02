# Tech Spec — way TUI design (WAY-8)

## Overview + Design Principles

This is a living design doc for WAY-8's first-principles design pass on
`way`'s TUI: building outward from the current PoC-shaped UI toward an
end-state design, rather than continuing to bolt features on incrementally.
Principles are extracted one at a time, in conversation with nick, grounded
in concrete scenarios rather than abstract values talk — same method already
validated for `way`'s README principles section. Each is recorded here as
it's settled, not drafted wholesale up front.

### 1. A task's lineage is its context — you inherit the why, not just the work.

Established 2026-07-26, from a concrete scenario: a 4-person team (nick,
Jordan, Takoda, Charlie) spent 90 days building a fresh IAM stack for ngrok
and is now splitting into two pairs for the next phase — Jordan/Charlie on
IdP/SSO, nick/Takoda on PATs release. The primary collaboration moment isn't
live co-presence, it's someone (Takoda) picking up a task cold, without
nick handing it off directly.

The actual gap in that moment isn't "is a session in flight" — it's "why is
this task scoped this way, what got decided before I saw it." Pivotal
decisions currently get posted to Slack; the goal is for `way` to carry that
decision trail on the task itself (lineage: parent task → breakdown →
sub-tasks, with rationale attached) so picking up a task never requires
asking someone or digging through a Slack thread. Dev flow stays
uninterrupted; Slack threads remain referenceable, not duplicated or
replaced.

(more principles to be added here as the design interview continues)
