<div class="row justify-content-center">
    <div class="col-md-8">
        <div class="card mb-4">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0">Your Profile</h4>
            </div>
            <div class="card-body">
                <form action="/user/profile" method="post">
                    <div class="mb-3">
                        <label for="name" class="form-label">Full Name</label>
                        <input type="text" class="form-control" id="name" name="name" value="<?= htmlspecialchars($user->getName()) ?>" required>
                    </div>
                    <div class="mb-3">
                        <label for="email" class="form-label">Email</label>
                        <input type="email" class="form-control" id="email" value="<?= htmlspecialchars($user->getEmail()) ?>" readonly disabled>
                        <div class="form-text">Email address cannot be changed.</div>
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary">Update Profile</button>
                    </div>
                </form>
            </div>
        </div>
        
        <div class="card">
            <div class="card-header bg-secondary text-white">
                <h4 class="mb-0">Change Password</h4>
            </div>
            <div class="card-body">
                <form action="/user/profile" method="post">
                    <div class="mb-3">
                        <label for="current_password" class="form-label">Current Password</label>
                        <input type="password" class="form-control" id="current_password" name="current_password" required>
                    </div>
                    <div class="mb-3">
                        <label for="new_password" class="form-label">New Password</label>
                        <input type="password" class="form-control" id="new_password" name="new_password" required minlength="8">
                    </div>
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label">Confirm New Password</label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" required minlength="8">
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-secondary">Change Password</button>
                    </div>
                </form>
            </div>
        </div>
        
        <div class="mt-3 text-center">
            <a href="/user/dashboard" class="btn btn-outline-primary">Back to Dashboard</a>
        </div>
    </div>
</div>