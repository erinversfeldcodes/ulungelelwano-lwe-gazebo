-- Dictionary metadata for a target-language word/phrase, keyed by its exact
-- (trimmed) spelling rather than by any particular card — cards.target_text
-- has no structural identity of its own today. Deliberately language-agnostic
-- (no noun class/transitivity/etc, unlike IsiXhosa-click's ExistingWord):
-- part_of_speech is a flat, unconstrained TEXT column (same convention as
-- cards.card_type), not an enum, since the target language is still undecided.
--
-- UNIQUE on target_text is a deliberate v1 simplification: it can't model two
-- unrelated senses sharing a spelling (homonyms) — accepted, not an oversight.
CREATE TABLE words (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_text TEXT NOT NULL UNIQUE,
    part_of_speech TEXT,
    notes TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One example-sentence pair per row (target sentence + its English/source
-- gloss). A child table so a word can carry more than one example later
-- without another migration, even though v1's own UI only ever writes 0 or 1.
CREATE TABLE word_examples (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    word_id UUID NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    target_sentence TEXT NOT NULL,
    gloss_sentence TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX word_examples_word_id_idx ON word_examples (word_id);

-- A relation between two words, mirroring IsiXhosa-click's linked_words in
-- spirit only (kept to one simple v1 type, not its language-specific
-- variants). link_type is flat TEXT, same convention as card_type/direction.
-- Treated as an unordered pair: looked up via "word_id = $1 OR
-- linked_word_id = $1"; saving a word's related-words list replaces every
-- row touching it from either side (see db::upsert_word_definition).
CREATE TABLE word_links (
    word_id UUID NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    linked_word_id UUID NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL DEFAULT 'related',
    PRIMARY KEY (word_id, linked_word_id)
);
