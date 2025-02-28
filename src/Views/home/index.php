<div class="row justify-content-center">
    <div class="col-md-8 text-center">
        <h1 class="display-4 mb-4"><?= $t('app.name') ?></h1>
        <p class="lead mb-5"><?= $t('app.tagline') ?></p>
        
        <div class="row mt-5">
            <div class="col-md-6">
                <div class="card mb-4">
                    <div class="card-body">
                        <h5 class="card-title"><?= $t('auth.login') ?></h5>
                        <p class="card-text">Already have an account? Sign in to access your Secret Santa groups.</p>
                        <a href="/auth/login" class="btn btn-primary"><?= $t('auth.login') ?></a>
                    </div>
                </div>
            </div>
            <div class="col-md-6">
                <div class="card mb-4">
                    <div class="card-body">
                        <h5 class="card-title"><?= $t('auth.register') ?></h5>
                        <p class="card-text">New to Secret Santa? Create an account to start organizing gift exchanges.</p>
                        <a href="/auth/register" class="btn btn-success"><?= $t('auth.register') ?></a>
                    </div>
                </div>
            </div>
        </div>
        
        <div class="mt-5">
            <h2>How It Works</h2>
            <div class="row mt-4">
                <div class="col-md-4">
                    <div class="card h-100">
                        <div class="card-body">
                            <h5 class="card-title">1. Create a Group</h5>
                            <p class="card-text">Start by creating a Secret Santa group and inviting your friends, family, or colleagues.</p>
                        </div>
                    </div>
                </div>
                <div class="col-md-4">
                    <div class="card h-100">
                        <div class="card-body">
                            <h5 class="card-title">2. Make Your Wishlist</h5>
                            <p class="card-text">Each participant creates a wishlist of items they'd like to receive.</p>
                        </div>
                    </div>
                </div>
                <div class="col-md-4">
                    <div class="card h-100">
                        <div class="card-body">
                            <h5 class="card-title">3. Secret Draw</h5>
                            <p class="card-text">Our system randomly assigns gift givers and receivers, keeping it a surprise!</p>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>
</div>