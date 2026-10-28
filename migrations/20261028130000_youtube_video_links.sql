-- A video card's media can be either an uploaded/recorded file (video_url,
-- rendered as a plain <video> element) or a linked YouTube video (rendered
-- as YouTube's iframe embed instead) — different enough at render time
-- that these need separate columns rather than overloading video_url with
-- two kinds of value and sniffing which one it is every time it's shown.
ALTER TABLE cards ADD COLUMN youtube_url TEXT;
