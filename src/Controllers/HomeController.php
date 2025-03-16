<?php

namespace SecretSanta\Controllers;

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
        // Store the language preference in session
        $this->session->set('locale', $locale);

        // Redirect back to referring page or home
        $referer = $this->request->getServerParam('HTTP_REFERER', '/');
        $this->redirect($referer);
    }
}
