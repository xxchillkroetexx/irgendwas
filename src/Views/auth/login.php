<?php
/**
 * Login Form View
 *
 * Displays a form that allows users to log in to their accounts.
 * The form includes fields for email, password, and a remember me option.
 * Also provides links to password reset and registration pages.
 *
 * @uses t() Translation function for internationalization
 */
?>
<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('auth.login.title') ?></h4>
            </div>
            <div class="card-body">
                <!-- Login form with POST submission -->
                <form action="/auth/login" method="post">
                    <!-- Email input field -->
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= t('auth.login.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required>
                    </div>
                    
                    <!-- Password input field -->
                    <div class="mb-3">
                        <label for="password" class="form-label"><?= t('auth.login.password') ?></label>
                        <input type="password" class="form-control" id="password" name="password" required>
                    </div>
                    
                    <!-- Remember me checkbox -->
                    <div class="mb-3 form-check">
                        <input type="checkbox" class="form-check-input" id="remember" name="remember">
                        <label class="form-check-label" for="remember"><?= t('auth.login.remember') ?></label>
                    </div>
                    
                    <!-- Submit button -->
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('auth.login.submit') ?></button>
                    </div>
                </form>

                <!-- Forgot password link -->
                <div class="mt-3 text-center">
                    <a href="/auth/forgot-password"><?= t('auth.login.forgotPassword') ?></a>
                </div>

                <hr>

                <!-- Registration option for new users -->
                <div class="text-center">
                    <p><?= t('auth.login.noAccount') ?></p>
                    <a href="/auth/register" class="btn btn-outline-success"><?= t('auth.register') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>