<?php

namespace SecretSanta\Controllers;

use SecretSanta\Repositories\UserRepository;

class AuthController extends BaseController {
    public function showLogin() {
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }
        
        return $this->render('auth/login');
    }
    
    public function login() {
        $email = $this->request->getPostParam('email');
        $password = $this->request->getPostParam('password');
        
        if (empty($email) || empty($password)) {
            $this->session->setFlash('error', 'Please enter both email and password');
            $this->redirect('/auth/login');
        }
        
        if ($this->auth->login($email, $password)) {
            $this->session->setFlash('success', $this->translator->translate('success.login_success'));
            $this->redirect('/user/dashboard');
        } else {
            $this->session->setFlash('error', $this->translator->translate('errors.invalid_credentials'));
            $this->redirect('/auth/login');
        }
    }
    
    public function showRegister() {
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }
        
        return $this->render('auth/register');
    }
    
    public function register() {
        $email = $this->request->getPostParam('email');
        $name = $this->request->getPostParam('name');
        $password = $this->request->getPostParam('password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');
        
        // Validate inputs
        $errors = $this->request->validate([
            'email' => 'required|email',
            'name' => 'required|min:2',
            'password' => 'required|min:8',
            'password_confirm' => 'required|same:password'
        ]);
        
        if (!empty($errors)) {
            $this->session->setFlash('errors', $errors);
            $this->redirect('/auth/register');
        }
        
        // Check if email already exists
        $userRepository = new UserRepository();
        if ($userRepository->findByEmail($email)) {
            $this->session->setFlash('error', $this->translator->translate('errors.email_taken'));
            $this->redirect('/auth/register');
        }
        
        $user = $this->auth->register($email, $name, $password);
        
        if ($user) {
            $this->session->setFlash('success', $this->translator->translate('success.register_success'));
            $this->redirect('/user/dashboard');
        } else {
            $this->session->setFlash('error', 'Registration failed. Please try again');
            $this->redirect('/auth/register');
        }
    }
    
    public function logout() {
        $this->auth->logout();
        $this->session->setFlash('success', $this->translator->translate('success.logout_success'));
        $this->redirect('/');
    }
    
    public function showForgotPassword() {
        return $this->render('auth/forgot-password');
    }
    
    public function forgotPassword() {
        $email = $this->request->getPostParam('email');
        
        if (empty($email)) {
            $this->session->setFlash('error', 'Please enter your email address');
            $this->redirect('/auth/forgot-password');
        }
        
        $success = $this->auth->requestPasswordReset($email);
        
        // Always show success message to prevent email enumeration
        $this->session->setFlash('success', $this->translator->translate('success.password_reset_link_sent'));
        $this->redirect('/auth/login');
    }
    
    public function showResetPassword(string $token) {
        return $this->render('auth/reset-password', ['token' => $token]);
    }
    
    public function resetPassword() {
        $token = $this->request->getPostParam('token');
        $password = $this->request->getPostParam('password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');
        
        // Validate inputs
        if (empty($token) || empty($password) || empty($passwordConfirm)) {
            $this->session->setFlash('error', 'All fields are required');
            $this->redirect('/auth/reset-password/' . $token);
        }
        
        if ($password !== $passwordConfirm) {
            $this->session->setFlash('error', $this->translator->translate('errors.passwords_dont_match'));
            $this->redirect('/auth/reset-password/' . $token);
        }
        
        if (strlen($password) < 8) {
            $this->session->setFlash('error', 'Password must be at least 8 characters long');
            $this->redirect('/auth/reset-password/' . $token);
        }
        
        $success = $this->auth->resetPassword($token, $password);
        
        if ($success) {
            $this->session->setFlash('success', $this->translator->translate('success.password_reset_success'));
            $this->redirect('/auth/login');
        } else {
            $this->session->setFlash('error', 'Invalid or expired reset token');
            $this->redirect('/auth/forgot-password');
        }
    }
}