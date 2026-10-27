-- Hard-capped at 4 users (enforced in application code at signup, not here —
-- a declarative row-count check needs a trigger, which is more machinery
-- than this trust model warrants).
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Magic-link email auth. A token is minted per sign-in attempt and is
-- single-use (used_at) and short-lived (expires_at). user_id is nullable
-- because the email might belong to a brand-new signup, resolved at verify
-- time once we know the token is genuinely being redeemed.
CREATE TABLE login_tokens (
    token TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    user_id UUID REFERENCES users(id),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    token TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Shared card pool: every user sees every card, no ownership-based access
-- control. card_type is a flat string for now ('image_to_target' is the
-- only variant); more variants (sound_to_written, sentence_translation)
-- arrive as later migrations add their own columns/tables as needed.
CREATE TABLE cards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_type TEXT NOT NULL,
    image_url TEXT,
    target_text TEXT NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE card_tags (
    card_id UUID NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (card_id, tag)
);

CREATE INDEX card_tags_tag_idx ON card_tags (tag);

-- SM-2 scheduling state, one row per (user, card). ease_factor/interval_days
-- follow the classic SM-2 algorithm; due_at is what review queries filter on.
CREATE TABLE review_state (
    user_id UUID NOT NULL REFERENCES users(id),
    card_id UUID NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    ease_factor REAL NOT NULL DEFAULT 2.5,
    interval_days REAL NOT NULL DEFAULT 0,
    repetitions INTEGER NOT NULL DEFAULT 0,
    due_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_reviewed_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, card_id)
);

CREATE INDEX review_state_due_at_idx ON review_state (user_id, due_at);
