ALTER TABLE Battle_Stats
    ADD COLUMN IF NOT EXISTS lose_streak_current smallint NOT NULL DEFAULT 0 CHECK (lose_streak_current >= 0),
    ADD COLUMN IF NOT EXISTS lose_streak_max     smallint NOT NULL DEFAULT 0 CHECK (lose_streak_max >= lose_streak_current);

CREATE OR REPLACE FUNCTION update_lose_streak_max_if_needed()
    RETURNS TRIGGER
    LANGUAGE PLPGSQL
AS $$
BEGIN
    IF NEW.lose_streak_current > NEW.lose_streak_max THEN
        NEW.lose_streak_max = NEW.lose_streak_current;
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE TRIGGER trg_update_lose_streak_max_if_needed BEFORE INSERT OR UPDATE ON Battle_Stats
    FOR EACH ROW EXECUTE FUNCTION update_lose_streak_max_if_needed();
