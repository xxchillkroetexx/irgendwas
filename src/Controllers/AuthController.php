<?php

namespace SecretSanta\Controllers;

use SecretSanta\Repositories\UserRepository;
use SecretSanta\Services\EmailService;

class AuthController extends BaseController
{
    public function showLogin()
    {
        // If already logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('auth/login');
    }

    public function login()
    {
        $email = $this->request->getPostParam('email');
        $password = $this->request->getPostParam('password');

        if (empty($email) || empty($password)) {
            $this->session->setFlash('error', t('flash.error.email_password_required'));
            $this->redirect('/auth/login');
            return;
        }

        if ($this->auth->login($email, $password)) {
            $this->session->setFlash('success', t('flash.success.logged_in'));
            $this->redirect('/user/dashboard');
        } else {
            $this->session->setFlash('error', t('flash.error.invalid_credentials'));
            $this->redirect('/auth/login');
        }
    }

    public function showRegister()
    {
        // If already logged in, redirect to dashboard
        if ($this->auth->check()) {
            $this->redirect('/user/dashboard');
        }

        return $this->render('auth/register');
    }

    public function register()
    {
        $email = $this->request->getPostParam('email');
        $name = $this->request->getPostParam('name');
        $password = $this->request->getPostParam('password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');
        $mailer = new EmailService();

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
        // To mitigate timing attacks, perform the same operations regardless of user existence
        $userRepository = new UserRepository();
        $userExists = $userRepository->findByEmail($email);

        if ($userExists) {
            $mailer->sendExistingAccountNotification($email);
        } else {
            // Create new user
            $this->auth->register($email, $name, $password);
            $mailer->sendWelcomeEmail($email);
        }

        // Use the same response for both outcomes
        $this->session->setFlash('success', t('flash.success.registration_instructions'));

        // Add a small random delay to make timing analysis more difficult
        usleep(random_int(100000, 200000)); // 0.1-0.2 second delay

        $this->redirect('/auth/login');
    }

    public function logout()
    {
        $this->auth->logout();
        $this->session->setFlash('success', t('flash.success.logged_out'));
        $this->redirect('/');
    }

    public function showForgotPassword()
    {
        return $this->render('auth/forgot-password');
    }

    public function forgotPassword()
    {
        $email = $this->request->getPostParam('email');

        if (empty($email)) {
            $this->session->setFlash('error', t('flash.error.enter_email'));
            $this->redirect('/auth/forgot-password');
            return;
        }

        // Try to send password reset email
        $success = $this->auth->requestPasswordReset($email);

        // Always show the success message (to prevent email enumeration)
        $this->session->setFlash('success', t('flash.success.reset_instructions'));
        $this->redirect('/auth/login');
    }

    public function showResetPassword($token)
    {
        // Check if token is valid before showing the form
        $userRepository = new UserRepository();
        $user = $userRepository->findByResetToken($token);

        // If token doesn't exist or is expired, redirect to forgot password page with error
        if (
            !$user ||
            $user->getResetTokenExpires() === null ||
            strtotime($user->getResetTokenExpires()) < time()
        ) {
            $this->session->setFlash('error', t('flash.error.reset_token_expired'));
            return $this->redirect('/auth/forgot-password');
        }

        return $this->render('auth/reset-password', ['token' => $token]);
    }

    public function resetPassword()
    {
        $token = $this->request->getPostParam('token');
        $password = $this->request->getPostParam('password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');

        // Validate inputs
        if (empty($token) || empty($password) || empty($passwordConfirm)) {
            $this->session->setFlash('error', t('flash.error.all_fields_required'));
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        if ($password !== $passwordConfirm) {
            $this->session->setFlash('error', t('flash.error.passwords_dont_match'));
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        if (strlen($password) < 8) {
            $this->session->setFlash('error', t('flash.error.password_too_short'));
            $this->redirect('/auth/reset-password/' . $token);
            return;
        }

        $success = $this->auth->resetPassword($token, $password);

        if ($success) {
            $this->session->setFlash('success', t('flash.success.password_reset'));
            $this->redirect('/auth/login');
        } else {
            $this->session->setFlash('error', t('flash.error.invalid_reset_token'));
            $this->redirect('/auth/forgot-password');
        }
    }
}
