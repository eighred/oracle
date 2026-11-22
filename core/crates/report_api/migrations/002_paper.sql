BEGIN;
CREATE ROLE oracle_paper_operator NOLOGIN;
CREATE SCHEMA oracle_paper;
REVOKE ALL ON SCHEMA oracle_paper FROM PUBLIC;
GRANT USAGE ON SCHEMA oracle_paper TO oracle_paper_operator;

CREATE TABLE oracle_paper.experiments (
    first_ms bigint PRIMARY KEY CHECK (first_ms > 0 AND first_ms % 60000 = 0),
    frozen_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint,
    manifest bytea NOT NULL,
    CHECK (frozen_ms < first_ms)
);
CREATE TABLE oracle_paper.events (
    first_ms bigint NOT NULL REFERENCES oracle_paper.experiments,
    seq integer NOT NULL CHECK (seq BETWEEN 0 AND 8),
    kind text NOT NULL CHECK (kind IN ('sample', 'missed', 'canceled')),
    recorded_ms bigint NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint,
    payload bytea NOT NULL,
    CHECK (kind <> 'sample' OR (recorded_ms >= first_ms + seq * 60000
        AND recorded_ms < first_ms + seq * 60000 + 15000)),
    PRIMARY KEY (first_ms, seq)
);
GRANT SELECT ON oracle_paper.experiments, oracle_paper.events TO oracle_paper_operator;
GRANT INSERT (first_ms, manifest) ON oracle_paper.experiments TO oracle_paper_operator;
GRANT INSERT (first_ms, seq, kind, payload) ON oracle_paper.events TO oracle_paper_operator;
GRANT USAGE ON SCHEMA oracle_paper TO oracle_report_writer;
GRANT SELECT ON oracle_paper.experiments, oracle_paper.events TO oracle_report_writer;
ALTER TABLE oracle_read.reports ADD COLUMN kind text NOT NULL DEFAULT 'historical'
    CHECK (kind IN ('historical','paper'));
GRANT INSERT (kind) ON oracle_read.reports TO oracle_report_writer;
COMMIT;
