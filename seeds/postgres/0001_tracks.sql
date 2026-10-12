-- Demo tracks: fixed UUIDv7 ids, time-ordered, so the newest-first list is stable.
-- Re-running this file changes nothing (ON CONFLICT (id) DO NOTHING).
INSERT INTO tracks (id, title, artist, duration_ms) VALUES
    ('0192f1a2-0000-7000-8000-000000000001', 'Clair de lune', 'Claude Debussy', 300000),
    ('0192f1a2-ea60-7000-8000-000000000002', 'Gymnopedie No. 1', 'Erik Satie', 190000),
    ('0192f1a3-d4c0-7000-8000-000000000003', 'Moonlight Sonata', 'Ludwig van Beethoven', 360000),
    ('0192f1a4-bf20-7000-8000-000000000004', 'The Four Seasons: Spring', 'Antonio Vivaldi', 210000),
    ('0192f1a5-a980-7000-8000-000000000005', 'Take Five', 'The Dave Brubeck Quartet', 324000),
    ('0192f1a6-93e0-7000-8000-000000000006', 'So What', 'Miles Davis', 562000),
    ('0192f1a7-7e40-7000-8000-000000000007', 'Blue in Green', 'Miles Davis', 337000),
    ('0192f1a8-68a0-7000-8000-000000000008', 'A Love Supreme, Pt. I', 'John Coltrane', 451000),
    ('0192f1a9-5300-7000-8000-000000000009', 'Giant Steps', 'John Coltrane', 287000),
    ('0192f1aa-3d60-7000-8000-00000000000a', 'Autumn Leaves', 'Bill Evans', 321000),
    ('0192f1ab-27c0-7000-8000-00000000000b', 'Bohemian Rhapsody', 'Queen', 354000),
    ('0192f1ac-1220-7000-8000-00000000000c', 'Hotel California', 'Eagles', 391000),
    ('0192f1ac-fc80-7000-8000-00000000000d', 'Stairway to Heaven', 'Led Zeppelin', 482000),
    ('0192f1ad-e6e0-7000-8000-00000000000e', 'Imagine', 'John Lennon', 187000),
    ('0192f1ae-d140-7000-8000-00000000000f', 'Hey Jude', 'The Beatles', 431000),
    ('0192f1af-bba0-7000-8000-000000000010', 'Billie Jean', 'Michael Jackson', 294000),
    ('0192f1b0-a600-7000-8000-000000000011', 'Smells Like Teen Spirit', 'Nirvana', 301000),
    ('0192f1b1-9060-7000-8000-000000000012', 'Wonderwall', 'Oasis', 258000),
    ('0192f1b2-7ac0-7000-8000-000000000013', 'Paranoid Android', 'Radiohead', 386000),
    ('0192f1b3-6520-7000-8000-000000000014', 'Karma Police', 'Radiohead', 264000),
    ('0192f1b4-4f80-7000-8000-000000000015', 'Starman', 'David Bowie', 254000),
    ('0192f1b5-39e0-7000-8000-000000000016', 'Heroes', 'David Bowie', 371000),
    ('0192f1b6-2440-7000-8000-000000000017', 'Redemption Song', 'Bob Marley', 229000),
    ('0192f1b7-0ea0-7000-8000-000000000018', 'Respect', 'Aretha Franklin', 147000),
    ('0192f1b7-f900-7000-8000-000000000019', 'What a Wonderful World', 'Louis Armstrong', 139000)
ON CONFLICT (id) DO NOTHING;
