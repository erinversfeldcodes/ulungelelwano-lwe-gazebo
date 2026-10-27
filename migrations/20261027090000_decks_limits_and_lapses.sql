-- Custom decks: a curated, many-to-many subset of the shared card pool you
-- can review as a unit. Distinct from card_tags (descriptive, flexible
-- labels) — a deck is something you deliberately build and review.
CREATE TABLE decks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE deck_cards (
    deck_id UUID NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
    card_id UUID NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    PRIMARY KEY (deck_id, card_id)
);

-- User-configurable cap on how many never-reviewed cards get introduced
-- per day, matching Anki's own (also user-set, not auto-tuned) approach.
-- NULL means unlimited.
ALTER TABLE users ADD COLUMN daily_new_card_limit INTEGER DEFAULT 20;

-- Counts times a card already in review (repetitions > 0) was failed
-- ("Again"), as distinct from a brand-new card's first attempt. Surfaced
-- on the stats dashboard as "cards you find hard".
ALTER TABLE review_state ADD COLUMN lapses INTEGER NOT NULL DEFAULT 0;

-- Set once, on a card's first-ever review by a user, and never touched
-- again — distinct from last_reviewed_at, which moves every time. This is
-- what the daily new-card limit counts against: without it, "repetitions
-- just went to 1" can't tell a truly new card apart from one that lapsed
-- back to the start of learning and already graduated once today.
ALTER TABLE review_state ADD COLUMN introduced_at TIMESTAMPTZ;

-- Every individual grading event, for stats (reviews done today, streaks,
-- accuracy) that review_state's single latest-state-per-card row can't
-- answer on its own.
CREATE TABLE review_log (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    card_id UUID NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    grade TEXT NOT NULL,
    graded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX review_log_user_graded_at_idx ON review_log (user_id, graded_at);
