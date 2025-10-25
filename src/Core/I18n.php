<?php

namespace SecretSanta\Core;

/**
 * I18n class provides internationalization and localization functionality.
 *
 * This singleton class handles translations, locale management, and formatting
 * of dates, numbers, and currencies according to the current locale.
 */
class I18n
{
    /** @var self|null Singleton instance of the I18n class */
    private static ?self $instance = null;
    
    /** @var string Current locale code (e.g., 'en', 'de') */
    private string $locale;
    
    /** @var array Loaded translations keyed by domain and message key */
    private array $translations = [];
    
    /** @var array Tracks which translation domains have been loaded */
    private array $loadedDomains = [];
    
    /**
     * Private constructor to enforce singleton pattern.
     * 
     * Sets the default locale from session or environment variable
     * and initializes PHP and ICU locales.
     */
    private function __construct()
    {
        // Set the default locale from session or environment
        $session = Session::getInstance();
        $this->locale = $session->get('locale', getenv('APP_LOCALE') ?: 'en');
        
        // Set locale for PHP functions
        setlocale(LC_ALL, $this->locale . '_' . strtoupper($this->locale) . '.UTF-8');
        
        // Initialize ICU
        \Locale::setDefault($this->locale);
    }
    
    /**
     * Gets the singleton instance of the I18n class.
     * 
     * @return self The I18n instance
     */
    public static function getInstance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        
        return self::$instance;
    }
    
    /**
     * Sets the current locale.
     * 
     * Updates PHP locale, ICU locale, and stores in session.
     * Clears loaded translations to ensure they're reloaded with new locale.
     * 
     * @param string $locale The locale code to set (e.g., 'en', 'de')
     */
    public function setLocale(string $locale): void
    {
        $this->locale = $locale;
        
        // Update PHP locale
        setlocale(LC_ALL, $locale . '_' . strtoupper($locale) . '.UTF-8');
        
        // Update ICU locale
        \Locale::setDefault($locale);
        
        // Save to session
        $session = Session::getInstance();
        $session->set('locale', $locale);
        
        // Clear loaded translations to force reload with new locale
        $this->loadedDomains = [];
        $this->translations = [];
    }
    
    /**
     * Gets the current locale code.
     * 
     * @return string Current locale code
     */
    public function getLocale(): string
    {
        return $this->locale;
    }
    
    /**
     * Gets the list of available locales supported by the application.
     * 
     * @return array List of locale codes
     */
    public function getAvailableLocales(): array
    {
        return ['en', 'de']; // Supported languages
    }
    
    /**
     * Translates a message key to the current locale.
     * 
     * Supports parameter replacement using {param} syntax.
     * 
     * @param string $key The translation key to look up
     * @param array $params Parameters to replace in the translated message
     * @param string $domain The translation domain (defaults to 'messages')
     * @return string The translated message or the key itself if not found
     */
    public function translate(string $key, array $params = [], string $domain = 'messages'): string
    {
        // Load domain if not already loaded
        $this->loadDomainIfNeeded($domain);
        
        // Get translation
        $message = $this->translations[$domain][$key] ?? $key;
        
        // Replace parameters
        if (!empty($params)) {
            foreach ($params as $param => $value) {
                $message = str_replace('{' . $param . '}', $value, $message);
            }
        }
        
        return $message;
    }
    
    /**
     * Formats a date according to the current locale.
     * 
     * @param \DateTime $date The date to format
     * @param string $style Format style ('none', 'short', 'medium', 'long', 'full')
     * @return string The formatted date string
     */
    public function formatDate(\DateTime $date, string $style = 'medium'): string
    {
        $formatter = new \IntlDateFormatter(
            $this->locale,
            $this->getDateFormatterConstant($style),
            \IntlDateFormatter::NONE
        );
        
        return $formatter->format($date);
    }
    
    /**
     * Formats a date and time according to the current locale.
     * 
     * @param \DateTime $dateTime The date and time to format
     * @param string $dateStyle Date format style ('none', 'short', 'medium', 'long', 'full')
     * @param string $timeStyle Time format style ('none', 'short', 'medium', 'long', 'full')
     * @return string The formatted date and time string
     */
    public function formatDateTime(\DateTime $dateTime, string $dateStyle = 'medium', string $timeStyle = 'short'): string
    {
        $formatter = new \IntlDateFormatter(
            $this->locale,
            $this->getDateFormatterConstant($dateStyle),
            $this->getDateFormatterConstant($timeStyle)
        );
        
        return $formatter->format($dateTime);
    }
    
    /**
     * Formats a number according to the current locale.
     * 
     * @param float $number The number to format
     * @param int $decimals Number of decimal places
     * @return string The formatted number string
     */
    public function formatNumber(float $number, int $decimals = 0): string
    {
        $formatter = new \NumberFormatter($this->locale, \NumberFormatter::DECIMAL);
        $formatter->setAttribute(\NumberFormatter::FRACTION_DIGITS, $decimals);
        
        return $formatter->format($number);
    }
    
    /**
     * Formats a currency amount according to the current locale.
     * 
     * @param float $amount The amount to format
     * @param string $currency The currency code (e.g., 'EUR', 'USD')
     * @return string The formatted currency string
     */
    public function formatCurrency(float $amount, string $currency = 'EUR'): string
    {
        $formatter = new \NumberFormatter($this->locale, \NumberFormatter::CURRENCY);
        
        return $formatter->formatCurrency($amount, $currency);
    }
    
    /**
     * Loads a translation domain if not already loaded.
     * 
     * Attempts to load translations from the current locale's JSON file,
     * falling back to English if not available.
     * 
     * @param string $domain The translation domain to load
     */
    private function loadDomainIfNeeded(string $domain): void
    {
        if (isset($this->loadedDomains[$domain])) {
            return;
        }
        
        // Load translations from JSON file
        $file = APP_ROOT . '/src/Localization/' . $this->locale . '/' . $domain . '.json';
        
        if (file_exists($file)) {
            $json = file_get_contents($file);
            $this->translations[$domain] = json_decode($json, true) ?: [];
        } else {
            // Fallback to English
            $file = APP_ROOT . '/src/Localization/en/' . $domain . '.json';
            if (file_exists($file)) {
                $json = file_get_contents($file);
                $this->translations[$domain] = json_decode($json, true) ?: [];
            } else {
                $this->translations[$domain] = [];
            }
        }
        
        $this->loadedDomains[$domain] = true;
    }
    
    /**
     * Converts a string date formatter style to the corresponding IntlDateFormatter constant.
     * 
     * @param string $style Format style name
     * @return int The IntlDateFormatter constant
     */
    private function getDateFormatterConstant(string $style): int
    {
        return match (strtolower($style)) {
            'none' => \IntlDateFormatter::NONE,
            'short' => \IntlDateFormatter::SHORT,
            'medium' => \IntlDateFormatter::MEDIUM,
            'long' => \IntlDateFormatter::LONG,
            'full' => \IntlDateFormatter::FULL,
            default => \IntlDateFormatter::MEDIUM,
        };
    }
}
