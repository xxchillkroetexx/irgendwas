<?php

namespace SecretSanta\Controllers;

use SecretSanta\Core\I18n;

/**
 * Home Controller
 * 
 * Handles basic application routes like the homepage and language settings.
 * 
 * @package SecretSanta\Controllers
 */
class HomeController extends BaseController
{
    /**
     * Display the homepage
     * 
     * If the user is logged in, redirects to dashboard.
     * Otherwise, shows the public landing page.
     * 
     * @return string|void HTML content or redirect
     */
    public function index()
    {
        // If user is logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('home/index');
    }

    /**
     * Set application language/locale
     * 
     * Changes the application language setting if the requested locale
     * is supported, then redirects back to the previous page.
     * 
     * @param string $locale The language code to set (e.g., 'en', 'de')
     * @return void
     */
    public function setLanguage($locale)
    {
        // Update the locale
        $i18n = I18n::getInstance();
        
        // Validate the locale is supported
        $availableLocales = $i18n->getAvailableLocales();
        if (in_array($locale, $availableLocales)) {
            $i18n->setLocale($locale);
        }

        // Redirect back to referring page or home
        $referer = $this->request->getServerParam('HTTP_REFERER', '/');
        $this->redirect($referer);
    }
}
