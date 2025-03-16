<?php

namespace SecretSanta\Controllers;

use SecretSanta\Core\Request;
use SecretSanta\Core\View;
use SecretSanta\Core\Auth;
use SecretSanta\Core\Session;
use SecretSanta\Core\Router;
use SecretSanta\Core\I18n;

class BaseController
{
    protected Request $request;
    protected View $view;
    protected Auth $auth;
    protected Session $session;
    protected Router $router;
    protected I18n $i18n;

    public function __construct()
    {
        $this->request = Request::getInstance();
        $this->auth = Auth::getInstance();
        $this->session = Session::getInstance();
        $this->router = Router::getInstance();
        $this->i18n = I18n::getInstance();

        $this->view = new View(__DIR__ . '/../Views');
    }

    protected function render(string $template, array $data = []): string
    {
        // Add common data for all views
        $data['auth'] = $this->auth;
        $data['session'] = $this->session;
        $data['i18n'] = $this->i18n;

        return $this->view->renderWithLayout($template, 'layouts/main', $data);
    }

    protected function renderPartial(string $template, array $data = []): string
    {
        // Add i18n to partial views too
        $data['i18n'] = $this->i18n;
        return $this->view->render($template, $data);
    }

    protected function redirect(string $url): void
    {
        $this->router->redirect($url);
    }

    protected function requireAuth(): void
    {
        if (!$this->auth->check()) {
            $this->session->setFlash('error', __('auth_required'));
            $this->redirect('/auth/login');
        }
    }

    protected function json(array $data, int $status = 200): string
    {
        http_response_code($status);
        header('Content-Type: application/json');
        return json_encode($data);
    }
}
