-- Optional media attachments alongside the existing image_url: a card can
-- have any combination of image/audio/video, and the review UI shows
-- whichever are present as part of its prompt.
ALTER TABLE cards ADD COLUMN audio_url TEXT;
ALTER TABLE cards ADD COLUMN video_url TEXT;

-- Two-way cards get reviewed in both directions — the existing
-- media → target_text direction, and additionally target_text → media.
-- Each direction is scheduled and graded independently (Anki's "Basic
-- and reversed" behavior), so review_state needs a direction alongside
-- its existing (user, card) key.
ALTER TABLE cards ADD COLUMN two_way BOOLEAN NOT NULL DEFAULT false;

-- Existing rows are all the one direction reviewed so far.
ALTER TABLE review_state ADD COLUMN direction TEXT NOT NULL DEFAULT 'forward';
ALTER TABLE review_state DROP CONSTRAINT review_state_pkey;
ALTER TABLE review_state ADD CONSTRAINT review_state_pkey PRIMARY KEY (user_id, card_id, direction);

-- Review history should record which direction was graded too, for the
-- same reason review_state needs it — without this, stats can't tell a
-- two-way card's two directions apart, and there'd be no way to backfill
-- it later (review_log has no FSRS state to infer direction from).
ALTER TABLE review_log ADD COLUMN direction TEXT NOT NULL DEFAULT 'forward';
