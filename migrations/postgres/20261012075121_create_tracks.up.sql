CREATE TABLE tracks (
    id uuid PRIMARY KEY,
    title text NOT NULL,
    artist text NOT NULL,
    duration_ms integer NOT NULL,
    version bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tracks_title_length CHECK (char_length(title) BETWEEN 1 AND 200),
    CONSTRAINT tracks_artist_length CHECK (char_length(artist) BETWEEN 1 AND 200),
    CONSTRAINT tracks_duration_ms_range CHECK (duration_ms BETWEEN 1 AND 86400000)
);
