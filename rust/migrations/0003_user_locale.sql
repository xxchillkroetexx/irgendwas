-- 0003 user locale (i18n-foundation): the account language.
-- NULL = no stored choice (mails use APP_LOCALE, login adopts nothing). The code writes only
-- canonical codes (en, de); no CHECK constraint, readers treat unknown values like NULL.
ALTER TABLE users ADD COLUMN locale VARCHAR(8) NULL DEFAULT NULL;
