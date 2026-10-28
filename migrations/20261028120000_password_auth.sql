-- Password auth replaces magic-link auth entirely. password_hash is
-- nullable so the app's one existing production user — who signed up via
-- magic link and has never set a password — keeps working: they get a
-- password the same way anyone who forgot theirs would, through the
-- password-reset flow below. A NULL hash must never match any login
-- attempt.
ALTER TABLE users ADD COLUMN password_hash TEXT;

-- Magic-link's login_tokens table is dropped in favor of a single-purpose
-- password_reset_tokens table. Unlike login_tokens, user_id is required:
-- a reset token is only ever minted for an email that already matches a
-- user (the request endpoint silently no-ops otherwise, to avoid leaking
-- account existence), so there's no "resolve the user at redeem time"
-- case left to support.
DROP TABLE login_tokens;

CREATE TABLE password_reset_tokens (
    token TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
