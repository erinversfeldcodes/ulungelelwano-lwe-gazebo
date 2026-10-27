-- Swaps the hand-rolled SM-2 scheduler for FSRS (crates.io `fsrs`, used by
-- modern Anki). ease_factor was SM-2's single ratchet; FSRS instead fits a
-- stability/difficulty pair per (user, card), both nullable only for a card
-- that's never been reviewed at all (FSRS fits a memory state on every
-- grade, including a first "Again", so any row with a review already
-- recorded always has both set). No production data exists yet on this
-- branch, so this is a straight swap rather than a value-preserving
-- migration.
ALTER TABLE review_state DROP COLUMN ease_factor;
ALTER TABLE review_state ADD COLUMN stability REAL;
ALTER TABLE review_state ADD COLUMN difficulty REAL;
