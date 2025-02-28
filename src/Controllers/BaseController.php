<?php

namespace SecretSanta\Controllers;

use SecretSanta\Core\Request;
use SecretSanta\Core\View;
use SecretSanta\Core\Auth;
use SecretSanta\Core\Session;
use SecretSanta\Core\Router;
use SecretSanta\Localization\Translator;

abstract class BaseController {
    protected Request $request;
    protected View $view;
    protected Auth $auth;
    protected Session $session;
    protected Router $router;
    protected Translator $translator;
    
    public function __construct() {
        $this->request = Request::getInstance();
        $this->auth = Auth::getInstance();
        $this->session = Session::getInstance();
        $this->router = Router::getInstance();
        $this->translator = Translator::getInstance();
        
        $this->view = new View(__DIR__ . '/../Views');
        
        // Add common view variables
        $this->view->assign('auth', $this->auth);
        $this->view->assign('session', $this->session);
        $this->view->assign('translator', $this->translator);
        $this->view->assign('t', function($key, $replacements = []) {
            return $this->translator->translate($key, $replacements);
        });
    }
    
    protected function render(string $template, array $data = []): string {
        return $this->view->renderWithLayout($template, 'layouts/main', $data);
    }
    
    protected function renderPartial(string $template, array $data = []): string {
        return $this->view->render($template, $data);
    }
    
    protected function redirect(string $url): void {
        $this->router->redirect($url);
    }
    
    protected function requireAuth(): void {
        if (!$this->auth->check()) {
            $this->session->setFlash('error', 'You must be logged in to access this page');
            $this->redirect('/auth/login');
        }
    }
    
    protected function json(array $data, int $status = 200): string {
        http_response_code($status);
        header('Content-Type: application/json');
        return json_encode($data);
    }
}