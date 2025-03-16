<?php

namespace SecretSanta\Controllers;

use SecretSanta\Core\I18n;

class HomeController extends BaseController
{
    public function index()
    {
        // If user is logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('home/index');
    }

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
