<?php

namespace SecretSanta\Controllers;

use SecretSanta\Repositories\UserRepository;

class AuthController extends BaseController {
    public function showLogin() {
        // If already logged in, redirect to dashboard
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
            return;
        }
        
        if ($this->auth->login($email, $password)) {
            $this->session->setFlash('success', 'You have successfully logged in');
            $this->redirect('/user/dashboard');
        } else {
            $this->session->setFlash('error', 'Invalid email or password');
            $this->redirect('/auth/login');
        }
    }
    
    public function showRegister() {
        // If already logged in, redirect to dashboard
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
        $errors = [];
        
        if (empty($email)) {
            $errors['email'] = 'Email is required';
        } elseif (!filter_var($email, FILTER_VALIDATE_EMAIL)) {
            $errors['email'] = 'Invalid email format';
        }
        
        if (empty($name)) {
            $errors['name'] = 'Name is required';
        }
        
        if (empty($password)) {
            $errors['password'] = 'Password is required';
        } elseif (strlen($password) < 8) {
            $errors['password'] = 'Password must be at least 8 characters';
        }
        
        if ($password !== $passwordConfirm) {
            $errors['password_confirm'] = 'Passwords do not match';
        }
        
        if (!empty($errors)) {
            $this->session->setFlash('errors', $errors);
            $this->session->setFlash('old_input', [
                'email' => $email,
                'name' => $name
            ]);
            $this->redirect('/auth/register');
            return;
        }
        
        // Check if email already exists
        $userRepository = new UserRepository();
        if ($userRepository->findByEmail($email)) {
            $this->session->setFlash('error', 'This email is already registered');
            $this->session->setFlash('old_input', [
                'email' => $email,
                'name' => $name
            ]);
            $this->redirect('/auth/register');
            return;
        }
        
        $user = $this->auth->register($email, $name, $password);
        
        if ($user) {
            $this->session->setFlash('success', 'You have successfully registered');
            $this->redirect('/user/dashboard');
        } else {
            $this->session->setFlash('error', 'Registration failed, please try again');
            $this->redirect('/auth/register');
        }
    }
    
    public function logout() {
        $this->auth->logout();
        $this->session->setFlash('success', 'You have been successfully logged out');
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
            return;
        }
        
        // Try to send password reset email
        $success = $this->auth->requestPasswordReset($email);
        
        // Always show the success message (to prevent email enumeration)
        $this->session->setFlash('success', 'If your email exists in our system, you will receive password reset instructions');
        $this->redirect('/auth/login');
    }
    
    public function showResetPassword($token) {
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
            return;
        }
        
        if ($password !== $passwordConfirm) {
            $this->session->setFlash('error', 'Passwords do not match');
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }
        
        if (strlen($password) < 8) {
            $this->session->setFlash('error', 'Password must be at least 8 characters');
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }
        
        $success = $this->auth->resetPassword($token, $password);
        
        if ($success) {
            $this->session->setFlash('success', 'Your password has been reset, you can now login');
            $this->redirect('/auth/login');
        } else {
            $this->session->setFlash('error', 'Invalid or expired reset token');
            $this->redirect('/auth/forgot-password');
        }
    }
}