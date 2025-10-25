<?php

/**
 * Global translation and localization functions for easier usage in templates.
 * 
 * This file provides a set of helper functions that wrap around the I18n singleton
 * to simplify internationalization tasks in template files.
 * 
 * @package SecretSanta\Localization
 */

use SecretSanta\Core\I18n;

if (!function_exists('t')) {
    /**
     * Translates a string using the localization system.
     * 
     * @param string $key    The translation key to look up in the translation files
     * @param array $params  Parameters to replace placeholders in the translated string
     * @param string $domain The translation domain (typically used to separate different translation categories)
     * 
     * @return string The translated string with all placeholders replaced
     */
    function t(string $key, array $params = [], string $domain = 'messages'): string
    {
        return I18n::getInstance()->translate($key, $params, $domain);
    }
}

if (!function_exists('format_date')) {
    /**
     * Formats a date according to the current locale settings.
     * 
     * @param \DateTime $date The date object to format
     * @param string $style   The formatting style ('full', 'long', 'medium', or 'short')
     * 
     * @return string The formatted date string in the current locale
     */
    function format_date(\DateTime $date, string $style = 'medium'): string
    {
        return I18n::getInstance()->formatDate($date, $style);
    }
}

if (!function_exists('format_datetime')) {
    /**
     * Formats a datetime object according to the current locale settings.
     * 
     * @param \DateTime $dateTime The datetime object to format
     * @param string $dateStyle   The formatting style for the date part ('full', 'long', 'medium', or 'short')
     * @param string $timeStyle   The formatting style for the time part ('full', 'long', 'medium', or 'short')
     * 
     * @return string The formatted datetime string in the current locale
     */
    function format_datetime(\DateTime $dateTime, string $dateStyle = 'medium', string $timeStyle = 'short'): string
    {
        return I18n::getInstance()->formatDateTime($dateTime, $dateStyle, $timeStyle);
    }
}

if (!function_exists('format_number')) {
    /**
     * Formats a numeric value according to the current locale settings.
     * 
     * @param float $number   The number to format
     * @param int $decimals   The number of decimal places to include
     * 
     * @return string The formatted number string in the current locale
     */
    function format_number(float $number, int $decimals = 0): string
    {
        return I18n::getInstance()->formatNumber($number, $decimals);
    }
}

if (!function_exists('format_currency')) {
    /**
     * Formats a monetary amount according to the current locale settings.
     * 
     * @param float $amount    The monetary amount to format
     * @param string $currency The ISO currency code (e.g., 'EUR', 'USD')
     * 
     * @return string The formatted currency string in the current locale
     */
    function format_currency(float $amount, string $currency = 'EUR'): string
    {
        return I18n::getInstance()->formatCurrency($amount, $currency);
    }
}

if (!function_exists('get_available_locales')) {
    /**
     * Retrieves the list of available locales supported by the application.
     * 
     * @return array An array of locale codes (e.g., ['en_US', 'de_DE', 'fr_FR'])
     */
    function get_available_locales(): array
    {
        return I18n::getInstance()->getAvailableLocales();
    }
}

if (!function_exists('get_current_locale')) {
    /**
     * Gets the currently active locale for the application.
     * 
     * @return string The current locale code (e.g., 'en_US')
     */
    function get_current_locale(): string
    {
        return I18n::getInstance()->getLocale();
    }
}
